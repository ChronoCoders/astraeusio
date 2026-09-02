use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tracing::warn;

use crate::{auth::AuthClaims, plan, routes::AppState};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn random_id() -> String {
    let bytes: [u8; 16] = rand::random();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn generate_key() -> String {
    let bytes: [u8; 32] = rand::random();
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("ast_{hex}")
}

fn sha256_hex(input: &str) -> String {
    Sha256::digest(input.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

// ── Request types ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateKeyRequest {
    pub name: String,
    /// Optional lifetime in days. Absent means the key does not expire, which
    /// is what every key created before this existed does.
    #[serde(default)]
    pub expires_in_days: Option<u32>,
}

/// Active keys one account may hold. Revoking a key frees a slot.
pub const MAX_KEYS_PER_USER: i64 = 10;

/// Longest lifetime a caller may ask for.
const MAX_EXPIRY_DAYS: u32 = 3650;

// ── Handlers ──────────────────────────────────────────────────────────────────

pub async fn create_api_key(
    State(s): State<AppState>,
    claims: AuthClaims,
    Json(body): Json<CreateKeyRequest>,
) -> Response {
    if let Some(r) = crate::routes::verified_gate(&s, &claims.sub).await {
        return r;
    }
    let user_plan = plan::resolve(&s.usage_counter, &s.db, &claims.sub).await;
    if !plan::satisfies(&user_plan, "developer") {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error":         "plan_required",
                "required_plan": "developer",
                "your_plan":     user_plan,
            })),
        )
            .into_response();
    }

    let name = body.name.trim().to_owned();
    if name.is_empty() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": "name must not be empty" })),
        )
            .into_response();
    }

    let expires_at = match body.expires_in_days {
        None => None,
        Some(days) if (1..=MAX_EXPIRY_DAYS).contains(&days) => {
            Some(chrono::Utc::now().timestamp() + i64::from(days) * 86_400)
        }
        Some(_) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({
                    "error": "invalid_expiry",
                    "message": "Key lifetime must be between 1 and 3650 days.",
                })),
            )
                .into_response();
        }
    };

    // Nothing capped how many keys an account could hold, so one account could
    // mint an unbounded number of long lived credentials.
    let active = match s.db.lock().await.count_active_api_keys(&claims.sub) {
        Ok(n) => n,
        Err(e) => {
            warn!("count_active_api_keys error: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
    };
    if active >= MAX_KEYS_PER_USER {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "key_limit_reached",
                "limit": MAX_KEYS_PER_USER,
                "message": "You have reached the limit of API keys. Revoke one to create another.",
            })),
        )
            .into_response();
    }

    let raw_key = generate_key();
    let key_hash = sha256_hex(&raw_key);
    let id = random_id();

    match s
        .writer
        .create_api_key(id.clone(), claims.sub, key_hash, name.clone(), expires_at)
        .await
    {
        Ok(()) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "id":  id,
                "key": raw_key,
                "name": name,
                "expires_at": expires_at,
            })),
        )
            .into_response(),
        Err(e) => {
            warn!("create_api_key error: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response()
        }
    }
}

pub async fn list_api_keys(State(s): State<AppState>, claims: AuthClaims) -> Response {
    match s.db.lock().await.list_api_keys(&claims.sub) {
        Ok(keys) => {
            let json: Vec<serde_json::Value> = keys
                .into_iter()
                .map(|k| {
                    serde_json::json!({
                        "id":            k.id,
                        "name":          k.name,
                        "created_at":    k.created_at,
                        "last_used_at":  k.last_used_at,
                        "request_count": k.request_count,
                        "expires_at":    k.expires_at,
                        "revoked_at":    k.revoked_at,
                    })
                })
                .collect();
            Json(serde_json::Value::Array(json)).into_response()
        }
        Err(e) => {
            warn!("list_api_keys error: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response()
        }
    }
}

pub async fn delete_api_key(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<String>,
) -> Response {
    match s.writer.revoke_api_key(id, claims.sub).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "key not found" })),
        )
            .into_response(),
        Err(e) => {
            warn!("delete_api_key error: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response()
        }
    }
}
