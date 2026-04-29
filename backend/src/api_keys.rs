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
}

// ── Handlers ──────────────────────────────────────────────────────────────────

pub async fn create_api_key(
    State(s): State<AppState>,
    claims: AuthClaims,
    Json(body): Json<CreateKeyRequest>,
) -> Response {
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

    let raw_key = generate_key();
    let key_hash = sha256_hex(&raw_key);
    let id = random_id();

    match s
        .writer
        .create_api_key(id.clone(), claims.sub, key_hash, name.clone())
        .await
    {
        Ok(()) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "id":  id,
                "key": raw_key,
                "name": name,
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
    match s.writer.delete_api_key(id, claims.sub).await {
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
