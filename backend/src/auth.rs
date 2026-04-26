use axum::{
    Json,
    extract::{FromRequestParts, State},
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Response},
};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::{db::DbError, routes::AppState};

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthClaims {
    pub sub: String,
    pub exp: usize,
}

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Serialize)]
struct LoginResponse {
    token: String,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

pub async fn register(State(s): State<AppState>, Json(body): Json<RegisterRequest>) -> Response {
    let password = body.password;
    let hash =
        match tokio::task::spawn_blocking(move || bcrypt::hash(password, bcrypt::DEFAULT_COST))
            .await
        {
            Ok(Ok(h)) => h,
            Ok(Err(e)) => {
                warn!("bcrypt hash error: {e}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "internal error" })),
                )
                    .into_response();
            }
            Err(e) => {
                warn!("spawn_blocking join error: {e}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "internal error" })),
                )
                    .into_response();
            }
        };

    match s.db.lock().await.create_user(&body.email, &hash) {
        Ok(()) => StatusCode::CREATED.into_response(),
        Err(DbError::EmailTaken) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "email already registered" })),
        )
            .into_response(),
        Err(e) => {
            warn!("create_user error: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response()
        }
    }
}

pub async fn login(State(s): State<AppState>, Json(body): Json<LoginRequest>) -> Response {
    let user = match s.db.lock().await.find_user_by_email(&body.email) {
        Ok(Some(u)) => u,
        Ok(None) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "invalid credentials" })),
            )
                .into_response();
        }
        Err(e) => {
            warn!("find_user error: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
    };

    let password = body.password;
    let hash = user.password_hash.clone();
    let valid = match tokio::task::spawn_blocking(move || bcrypt::verify(password, &hash)).await {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            warn!("bcrypt verify error: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
        Err(e) => {
            warn!("spawn_blocking join error: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
    };

    if !valid {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "invalid credentials" })),
        )
            .into_response();
    }

    let exp = (chrono::Utc::now().timestamp() + 86_400) as usize;
    let claims = AuthClaims {
        sub: user.email,
        exp,
    };

    match encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(s.jwt_secret.as_bytes()),
    ) {
        Ok(token) => Json(LoginResponse { token }).into_response(),
        Err(e) => {
            warn!("jwt encode error: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response()
        }
    }
}

pub async fn change_password(
    State(s): State<AppState>,
    claims: AuthClaims,
    Json(body): Json<ChangePasswordRequest>,
) -> Response {
    if body.new_password.len() < 8 {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": "new password must be at least 8 characters" })),
        )
            .into_response();
    }

    let user = match s.db.lock().await.find_user_by_email(&claims.sub) {
        Ok(Some(u)) => u,
        Ok(None) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "user not found" })),
            )
                .into_response();
        }
        Err(e) => {
            warn!("change_password find_user error: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
    };

    let current = body.current_password;
    let stored_hash = user.password_hash.clone();
    let valid = match tokio::task::spawn_blocking(move || bcrypt::verify(current, &stored_hash)).await {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            warn!("change_password bcrypt verify error: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
        Err(e) => {
            warn!("change_password spawn_blocking error: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
    };

    if !valid {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "current password is incorrect" })),
        )
            .into_response();
    }

    let new_pw = body.new_password;
    let new_hash = match tokio::task::spawn_blocking(move || bcrypt::hash(new_pw, bcrypt::DEFAULT_COST)).await {
        Ok(Ok(h)) => h,
        Ok(Err(e)) => {
            warn!("change_password bcrypt hash error: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
        Err(e) => {
            warn!("change_password spawn_blocking hash error: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
    };

    match s.db.lock().await.update_password_hash(&claims.sub, &new_hash) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            warn!("change_password update error: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response()
        }
    }
}

// ── JWT extractor ─────────────────────────────────────────────────────────────

impl FromRequestParts<AppState> for AuthClaims {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or_else(|| {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({ "error": "missing or invalid Authorization header" })),
                )
                    .into_response()
            })?;

        decode::<AuthClaims>(
            token,
            &DecodingKey::from_secret(state.jwt_secret.as_bytes()),
            &Validation::default(),
        )
        .map(|data| data.claims)
        .map_err(|e| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        })
    }
}
