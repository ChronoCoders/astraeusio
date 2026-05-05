use axum::{
    Json,
    extract::{FromRequestParts, Path, State},
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Response},
};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use totp_rs::{Algorithm, Secret, TOTP};
use tracing::warn;

use crate::{db::DbError, db_writer::WriteCmd, mailer, rate_limit, routes::AppState};

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum AuthType {
    #[default]
    Jwt,
    ApiKey,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthClaims {
    pub sub: String,
    pub exp: usize,
    #[serde(skip)]
    pub auth_type: AuthType,
}

/// Short-lived token with a `purpose` discriminant (verify_email / 2fa_partial).
#[derive(Serialize, Deserialize)]
struct PurposeClaims {
    sub: String,
    exp: usize,
    purpose: String,
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

#[derive(Deserialize)]
pub struct TotpCodeRequest {
    pub code: String,
}

#[derive(Deserialize)]
pub struct TotpLoginRequest {
    pub partial_token: String,
    pub code: String,
}

#[derive(Serialize)]
struct LoginResponse {
    token: String,
}

// ── Token helpers ─────────────────────────────────────────────────────────────

fn purpose_token(
    sub: &str,
    purpose: &str,
    ttl: i64,
    secret: &str,
) -> Result<String, jsonwebtoken::errors::Error> {
    let exp = (chrono::Utc::now().timestamp() + ttl) as usize;
    encode(
        &Header::default(),
        &PurposeClaims {
            sub: sub.to_string(),
            exp,
            purpose: purpose.to_string(),
        },
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

fn decode_purpose(token: &str, purpose: &str, secret: &str) -> Result<String, &'static str> {
    let claims = decode::<PurposeClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| "invalid or expired token")?
    .claims;
    if claims.purpose != purpose {
        return Err("wrong token purpose");
    }
    Ok(claims.sub)
}

// ── TOTP helpers ──────────────────────────────────────────────────────────────

fn build_totp(secret_b32: &str, account: &str) -> Result<TOTP, &'static str> {
    let bytes = Secret::Encoded(secret_b32.to_string())
        .to_bytes()
        .map_err(|_| "invalid totp secret")?;
    TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        bytes,
        Some("Astraeusio".to_string()),
        account.to_string(),
    )
    .map_err(|_| "totp construction failed")
}

fn check_totp(secret_b32: &str, account: &str, code: &str) -> Result<bool, &'static str> {
    let totp = build_totp(secret_b32, account)?;
    totp.check_current(code).map_err(|_| "system time error")
}

// ── Handlers ──────────────────────────────────────────────────────────────────

pub async fn register(State(s): State<AppState>, Json(body): Json<RegisterRequest>) -> Response {
    let email = body.email.clone();
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

    match s.writer.create_user(email.clone(), hash).await {
        Ok(()) => {
            // Fire verification email if mailer is configured.
            if let Some(ref mc) = s.mailer
                && let Ok(token) = purpose_token(&email, "verify_email", 86_400, &s.jwt_secret)
            {
                let url = format!("{}/verify-email?token={}", s.app_url, token);
                let mc = mc.clone();
                tokio::spawn(async move {
                    mailer::send_verification_email(&mc, &email, &url).await;
                });
            }
            StatusCode::CREATED.into_response()
        }
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

    // If 2FA is active, issue a short-lived partial token instead of a full JWT.
    if user.totp_enabled {
        match purpose_token(&user.email, "2fa_partial", 300, &s.jwt_secret) {
            Ok(partial) => {
                return Json(serde_json::json!({ "requires_2fa": true, "partial_token": partial }))
                    .into_response();
            }
            Err(e) => {
                warn!("2fa partial token error: {e}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "internal error" })),
                )
                    .into_response();
            }
        }
    }

    issue_jwt(&user.email, &s.jwt_secret)
}

pub async fn login_2fa(State(s): State<AppState>, Json(body): Json<TotpLoginRequest>) -> Response {
    let email = match decode_purpose(&body.partial_token, "2fa_partial", &s.jwt_secret) {
        Ok(e) => e,
        Err(e) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": e })),
            )
                .into_response();
        }
    };

    let user = match s.db.lock().await.find_user_by_email(&email) {
        Ok(Some(u)) => u,
        Ok(None) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "user not found" })),
            )
                .into_response();
        }
        Err(e) => {
            warn!("login_2fa find_user: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
    };

    let Some(ref secret) = user.totp_secret else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "2FA not configured" })),
        )
            .into_response();
    };

    match check_totp(secret, &email, &body.code) {
        Ok(true) => issue_jwt(&email, &s.jwt_secret),
        Ok(false) => (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "invalid code" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

// ── Email verification ─────────────────────────────────────────────────────────

pub async fn verify_email(Path(token): Path<String>, State(s): State<AppState>) -> Response {
    let email = match decode_purpose(&token, "verify_email", &s.jwt_secret) {
        Ok(e) => e,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": e })),
            )
                .into_response();
        }
    };

    match s.writer.set_email_verified(email).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            warn!("set_email_verified: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response()
        }
    }
}

pub async fn resend_verification(State(s): State<AppState>, claims: AuthClaims) -> Response {
    let Some(ref mc) = s.mailer else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "email service not configured" })),
        )
            .into_response();
    };

    let user = match s.db.lock().await.find_user_by_email(&claims.sub) {
        Ok(Some(u)) => u,
        _ => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
    };

    if user.email_verified {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "email already verified" })),
        )
            .into_response();
    }

    let token = match purpose_token(&claims.sub, "verify_email", 86_400, &s.jwt_secret) {
        Ok(t) => t,
        Err(e) => {
            warn!("token gen error: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
    };

    let url = format!("{}/verify-email?token={}", s.app_url, token);
    let mc = mc.clone();
    let email = claims.sub.clone();
    tokio::spawn(async move {
        mailer::send_verification_email(&mc, &email, &url).await;
    });

    StatusCode::NO_CONTENT.into_response()
}

// ── 2FA setup / verify / disable ─────────────────────────────────────────────

pub async fn setup_2fa(State(s): State<AppState>, claims: AuthClaims) -> Response {
    let user = match s.db.lock().await.find_user_by_email(&claims.sub) {
        Ok(Some(u)) => u,
        _ => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
    };

    if user.totp_enabled {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "2FA already enabled" })),
        )
            .into_response();
    }

    // Generate random secret and build TOTP.
    let raw_secret = Secret::generate_secret();
    let secret_b32 = match raw_secret.to_encoded() {
        Secret::Encoded(s) => s,
        _ => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "secret encoding failed" })),
            )
                .into_response();
        }
    };

    let secret_bytes = match Secret::Encoded(secret_b32.clone()).to_bytes() {
        Ok(b) => b,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
    };

    let totp = match TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret_bytes,
        Some("Astraeusio".to_string()),
        claims.sub.clone(),
    ) {
        Ok(t) => t,
        Err(e) => {
            warn!("totp new error: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
    };

    let qr_base64 = match totp.get_qr_base64() {
        Ok(q) => q,
        Err(e) => {
            warn!("qr gen error: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
    };

    // Store pending (unconfirmed) secret.
    if let Err(e) = s
        .writer
        .set_totp_secret(claims.sub, secret_b32.clone())
        .await
    {
        warn!("set_totp_secret: {e}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "internal error" })),
        )
            .into_response();
    }

    Json(serde_json::json!({
        "secret":  secret_b32,
        "qr_code": format!("data:image/png;base64,{}", qr_base64),
    }))
    .into_response()
}

pub async fn verify_2fa(
    State(s): State<AppState>,
    claims: AuthClaims,
    Json(body): Json<TotpCodeRequest>,
) -> Response {
    let user = match s.db.lock().await.find_user_by_email(&claims.sub) {
        Ok(Some(u)) => u,
        _ => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
    };

    if user.totp_enabled {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "2FA already enabled" })),
        )
            .into_response();
    }

    let Some(ref secret) = user.totp_secret else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "2FA setup not initiated" })),
        )
            .into_response();
    };

    match check_totp(secret, &claims.sub, &body.code) {
        Ok(true) => {}
        Ok(false) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "invalid code" })),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e })),
            )
                .into_response();
        }
    }

    match s.writer.enable_totp(claims.sub).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            warn!("enable_totp: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response()
        }
    }
}

pub async fn disable_2fa(
    State(s): State<AppState>,
    claims: AuthClaims,
    Json(body): Json<TotpCodeRequest>,
) -> Response {
    let user = match s.db.lock().await.find_user_by_email(&claims.sub) {
        Ok(Some(u)) => u,
        _ => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
    };

    if !user.totp_enabled {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "2FA is not enabled" })),
        )
            .into_response();
    }

    let Some(ref secret) = user.totp_secret else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "internal error" })),
        )
            .into_response();
    };

    match check_totp(secret, &claims.sub, &body.code) {
        Ok(true) => {}
        Ok(false) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "invalid code" })),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e })),
            )
                .into_response();
        }
    }

    match s.writer.disable_totp(claims.sub).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            warn!("disable_totp: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response()
        }
    }
}

// ── Change password ───────────────────────────────────────────────────────────

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
    let valid =
        match tokio::task::spawn_blocking(move || bcrypt::verify(current, &stored_hash)).await {
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
    let new_hash =
        match tokio::task::spawn_blocking(move || bcrypt::hash(new_pw, bcrypt::DEFAULT_COST)).await
        {
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

    match s.writer.update_password(claims.sub, new_hash).await {
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

// ── JWT / API-key extractor ────────────────────────────────────────────────────

fn sha256_hex(input: &str) -> String {
    Sha256::digest(input.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn issue_jwt(email: &str, secret: &str) -> Response {
    let exp = (chrono::Utc::now().timestamp() + 86_400) as usize;
    let claims = AuthClaims {
        sub: email.to_string(),
        exp,
        auth_type: AuthType::Jwt,
    };
    match encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
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

        // API key path — prefix "ast_"
        if token.starts_with("ast_") {
            let hash = sha256_hex(token);
            let sub_opt = {
                let db = state.db.lock().await;
                db.find_api_key_by_hash(&hash).map_err(|e| {
                    warn!("api_key lookup error: {e}");
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({ "error": "internal error" })),
                    )
                        .into_response()
                })?
            };
            if sub_opt.is_some() {
                state.writer.fire(WriteCmd::TouchApiKey(hash));
            }

            match sub_opt {
                Some(sub) => {
                    rate_limit::check_and_increment(&state.usage_counter, &state.db, &sub).await?;
                    return Ok(AuthClaims {
                        sub,
                        exp: usize::MAX,
                        auth_type: AuthType::ApiKey,
                    });
                }
                None => {
                    return Err((
                        StatusCode::UNAUTHORIZED,
                        Json(serde_json::json!({ "error": "invalid API key" })),
                    )
                        .into_response());
                }
            }
        }

        // JWT path
        let claims = decode::<AuthClaims>(
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
        })?;

        Ok(claims)
    }
}
