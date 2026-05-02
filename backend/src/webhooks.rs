use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use tracing::warn;

use crate::{auth::AuthClaims, plan, routes::AppState};

fn random_hex(n: usize) -> String {
    let bytes: Vec<u8> = (0..n).map(|_| rand::random::<u8>()).collect();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

const VALID_EVENTS: &[&str] = &[
    "kp_storm",
    "solar_wind_speed",
    "xray_flare",
    "asteroid_close",
    "ml_forecast_storm",
];

#[derive(Deserialize)]
pub struct CreateWebhookBody {
    pub url: String,
    pub events: Vec<String>,
}

pub async fn create_webhook(
    State(s): State<AppState>,
    claims: AuthClaims,
    Json(body): Json<CreateWebhookBody>,
) -> Response {
    let user_plan = plan::resolve(&s.usage_counter, &s.db, &claims.sub).await;
    if !plan::satisfies(&user_plan, "pro") {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error":         "plan_required",
                "required_plan": "pro",
                "your_plan":     user_plan,
            })),
        )
            .into_response();
    }

    let url = body.url.trim().to_owned();
    if url.is_empty() || (!url.starts_with("https://") && !url.starts_with("http://")) {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": "url must be a valid http or https URL" })),
        )
            .into_response();
    }

    let events: Vec<String> = body
        .events
        .into_iter()
        .filter(|e| VALID_EVENTS.contains(&e.as_str()))
        .collect();
    if events.is_empty() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": "at least one valid event is required" })),
        )
            .into_response();
    }

    let id = random_hex(16);
    let secret = random_hex(32);
    let events_json = serde_json::to_string(&events).unwrap_or_else(|_| "[]".to_owned());

    match s
        .writer
        .create_webhook(id.clone(), claims.sub, url, secret.clone(), events_json)
        .await
    {
        Ok(()) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "id":     id,
                "secret": secret,
                "events": events,
            })),
        )
            .into_response(),
        Err(e) => {
            warn!("create_webhook error: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response()
        }
    }
}

pub async fn list_webhooks(State(s): State<AppState>, claims: AuthClaims) -> Response {
    let user_plan = plan::resolve(&s.usage_counter, &s.db, &claims.sub).await;
    if !plan::satisfies(&user_plan, "pro") {
        return Json(serde_json::Value::Array(vec![])).into_response();
    }

    match s.db.lock().await.list_webhooks(&claims.sub) {
        Ok(hooks) => {
            let json: Vec<serde_json::Value> = hooks
                .into_iter()
                .map(|h| {
                    serde_json::json!({
                        "id":         h.id,
                        "url":        h.url,
                        "events":     h.events,
                        "created_at": h.created_at,
                    })
                })
                .collect();
            Json(serde_json::Value::Array(json)).into_response()
        }
        Err(e) => {
            warn!("list_webhooks error: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response()
        }
    }
}

pub async fn delete_webhook(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<String>,
) -> Response {
    match s.writer.delete_webhook(id, claims.sub).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "webhook not found" })),
        )
            .into_response(),
        Err(e) => {
            warn!("delete_webhook error: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response()
        }
    }
}
