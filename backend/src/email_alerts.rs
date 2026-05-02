use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use tracing::warn;

use crate::{auth::AuthClaims, plan, routes::AppState};

fn random_id() -> String {
    let bytes: [u8; 16] = rand::random();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Deserialize)]
pub struct EmailAlertBody {
    pub enabled: bool,
    pub kp_threshold: f64,
    pub wind_threshold: f64,
}

pub async fn get_email_alert(State(s): State<AppState>, claims: AuthClaims) -> Response {
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

    match s.db.lock().await.get_email_alert(&claims.sub) {
        Ok(Some(row)) => Json(serde_json::json!({
            "enabled":        row.enabled,
            "kp_threshold":   row.kp_threshold_e2 as f64 / 100.0,
            "wind_threshold": row.wind_threshold_e1 as f64 / 10.0,
        }))
        .into_response(),
        Ok(None) => Json(serde_json::json!({
            "enabled":        false,
            "kp_threshold":   5.0,
            "wind_threshold": 700.0,
        }))
        .into_response(),
        Err(e) => {
            warn!("get_email_alert error: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response()
        }
    }
}

pub async fn upsert_email_alert(
    State(s): State<AppState>,
    claims: AuthClaims,
    Json(body): Json<EmailAlertBody>,
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

    let kp = body.kp_threshold.clamp(1.0, 9.0);
    let wind = body.wind_threshold.clamp(100.0, 2000.0);
    let kp_e2 = (kp * 100.0).round() as i64;
    let wind_e1 = (wind * 10.0).round() as i64;
    let id = random_id();

    match s
        .writer
        .upsert_email_alert(id, claims.sub, body.enabled, kp_e2, wind_e1)
        .await
    {
        Ok(()) => Json(serde_json::json!({
            "enabled":        body.enabled,
            "kp_threshold":   kp,
            "wind_threshold": wind,
        }))
        .into_response(),
        Err(e) => {
            warn!("upsert_email_alert error: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response()
        }
    }
}
