use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, MutexGuard};

use anyhow::anyhow;
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use serde::Deserialize;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

use dashmap::DashMap;

use crate::{
    api_keys, auth,
    auth::{AuthClaims, AuthType},
    db::Store,
    db_writer::{DbWriterHandle, WriteCmd},
    email_alerts, mailer, plan,
    rate_limit::UsageCounter,
    webhooks,
};

// ── Cache ─────────────────────────────────────────────────────────────────────

type CacheMap = HashMap<&'static str, (Instant, serde_json::Value)>;

async fn cached<F, Fut>(
    cache: &Arc<Mutex<CacheMap>>,
    key: &'static str,
    ttl: Duration,
    fetch: F,
) -> Result<Json<serde_json::Value>, AppError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<serde_json::Value, AppError>>,
{
    {
        let guard = cache.lock().await;
        if let Some((ts, val)) = guard.get(key)
            && ts.elapsed() < ttl
        {
            return Ok(Json(val.clone()));
        }
    }
    let val = fetch().await?;
    cache
        .lock()
        .await
        .insert(key, (Instant::now(), val.clone()));
    Ok(Json(val))
}

// ── State ─────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AppState {
    pub client: reqwest::Client,
    pub db: Arc<Mutex<Store>>,
    pub writer: DbWriterHandle,
    pub ml_url: String,
    pub cache: Arc<Mutex<CacheMap>>,
    pub jwt_secret: String,
    pub usage_counter: Arc<UsageCounter>,
    pub mailer: Option<mailer::MailerConfig>,
    pub app_url: String,
}

impl AppState {
    pub fn new(
        client: reqwest::Client,
        db: Store,
        writer: DbWriterHandle,
        ml_url: String,
        jwt_secret: String,
        mailer: Option<mailer::MailerConfig>,
        app_url: String,
    ) -> Self {
        Self {
            client,
            db: Arc::new(Mutex::new(db)),
            writer,
            ml_url,
            cache: Arc::new(Mutex::new(HashMap::new())),
            jwt_secret,
            usage_counter: Arc::new(DashMap::new()),
            mailer,
            app_url,
        }
    }
}

// ── Error ─────────────────────────────────────────────────────────────────────

pub struct AppError(anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        tracing::error!("{}", self.0);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": self.0.to_string() })),
        )
            .into_response()
    }
}

impl<E: Into<anyhow::Error>> From<E> for AppError {
    fn from(e: E) -> Self {
        Self(e.into())
    }
}

async fn lock_db(db: &Arc<Mutex<Store>>) -> MutexGuard<'_, Store> {
    db.lock().await
}

/// Returns a 403 response if the user's plan doesn't meet `required`, else None.
async fn plan_gate(s: &AppState, email: &str, required: &'static str) -> Option<Response> {
    let user_plan = plan::resolve(&s.usage_counter, &s.db, email).await;
    if plan::satisfies(&user_plan, required) {
        return None;
    }
    Some(
        (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error":         "plan_required",
                "required_plan": required,
                "your_plan":     user_plan,
            })),
        )
            .into_response(),
    )
}

// ── Router ────────────────────────────────────────────────────────────────────

async fn health(State(s): State<AppState>) -> impl IntoResponse {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    // ── ML service ────────────────────────────────────────────────────────────
    let ml_status = match s
        .client
        .get(format!("{}/health", s.ml_url))
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => "operational",
        Ok(_) => "degraded",
        Err(_) => "degraded",
    };

    // ── DB freshness ─────────────────────────────────────────────────────────
    let (noaa_ts, nasa_ts, celestrak_ts) = lock_db(&s.db).await.health_freshness();

    fn component_status(
        last: Option<i64>,
        now: i64,
        stale_secs: i64,
    ) -> (&'static str, Option<i64>) {
        match last {
            None => ("unknown", None),
            Some(t) if now - t > stale_secs => ("degraded", Some(t)),
            Some(t) => ("operational", Some(t)),
        }
    }

    let (noaa_status, noaa_last) = component_status(noaa_ts, now, 600);
    let (nasa_status, nasa_last) = component_status(nasa_ts, now, 90_000);
    let (celestrak_status, celestrak_last) = component_status(celestrak_ts, now, 14_400);
    let db_status = if noaa_ts.is_some() {
        "operational"
    } else {
        "unknown"
    };

    // ── Overall ───────────────────────────────────────────────────────────────
    let statuses = [
        ml_status,
        db_status,
        noaa_status,
        nasa_status,
        celestrak_status,
    ];
    let overall = if statuses.iter().all(|&s| s == "operational") {
        "operational"
    } else {
        "degraded"
    };

    Json(serde_json::json!({
        "status":     overall,
        "checked_at": now,
        "components": {
            "backend_api": {
                "status":      "operational",
                "last_checked": now
            },
            "ml_forecast": {
                "status":      ml_status,
                "last_checked": now
            },
            "database": {
                "status":     db_status,
                "last_write": noaa_last
            },
            "noaa": {
                "status":      noaa_status,
                "last_update": noaa_last
            },
            "nasa": {
                "status":      nasa_status,
                "last_update": nasa_last
            },
            "celestrak": {
                "status":      celestrak_status,
                "last_update": celestrak_last
            }
        }
    }))
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/health", get(health))
        .route("/auth/register", post(auth::register))
        .route("/auth/login", post(auth::login))
        .route("/auth/change-password", post(auth::change_password))
        .route("/auth/verify-email/{token}", post(auth::verify_email))
        .route("/auth/resend-verification", post(auth::resend_verification))
        .route("/auth/forgot-password", post(auth::forgot_password))
        .route("/auth/reset-password", post(auth::reset_password))
        .route("/auth/2fa/setup", post(auth::setup_2fa))
        .route("/auth/2fa/verify", post(auth::verify_2fa))
        .route("/auth/2fa/disable", post(auth::disable_2fa))
        .route("/auth/2fa/login", post(auth::login_2fa))
        .route("/api/apod", get(get_apod))
        .route("/api/neo", get(get_neo))
        .route("/api/epic", get(get_epic))
        .route("/api/exoplanets", get(get_exoplanets))
        .route("/api/kp", get(get_kp))
        .route("/api/kp-3h", get(get_kp_3h))
        .route("/api/solar-wind", get(get_solar_wind))
        .route("/api/xray", get(get_xray))
        .route("/api/alerts", get(get_alerts))
        .route("/api/iss", get(get_iss))
        .route("/api/kp-forecast", get(get_kp_forecast))
        .route("/api/forecast/history", get(get_forecast_history))
        .route("/api/forecast/metrics", get(get_forecast_metrics))
        .route("/api/events", get(get_events))
        .route("/api/anomalies", get(get_anomalies))
        .route("/api/imf", get(get_imf))
        .route("/api/dst", get(get_dst))
        .route("/api/starlink", get(get_starlink))
        .route("/api/reports/summary", get(get_report_summary))
        .route("/api/reports/export", get(get_report_export))
        .route("/api/reports/kp", get(get_report_kp))
        .route("/api/reports/solar-wind", get(get_report_solar_wind))
        .route("/api/public/kp", get(get_public_kp))
        .route("/api/public/solar-wind", get(get_public_solar_wind))
        .route("/api/public/forecast", get(get_public_forecast))
        .route("/api/user/me", get(get_user_me))
        .route("/api/user/plan", post(update_user_plan))
        .route("/api/usage", get(get_usage))
        .route(
            "/api/keys",
            get(api_keys::list_api_keys).post(api_keys::create_api_key),
        )
        .route("/api/keys/{id}", delete(api_keys::delete_api_key))
        .route(
            "/api/webhooks",
            get(webhooks::list_webhooks).post(webhooks::create_webhook),
        )
        .route("/api/webhooks/{id}", delete(webhooks::delete_webhook))
        .route(
            "/api/email-alerts",
            get(email_alerts::get_email_alert).post(email_alerts::upsert_email_alert),
        )
        .route(
            "/api/custom-rules",
            get(list_custom_rules).post(create_custom_rule),
        )
        .route("/api/custom-rules/{id}", delete(delete_custom_rule))
        .route("/api/custom-rules/{id}/toggle", post(toggle_custom_rule))
        .route("/mcp", post(mcp_handler))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(state)
}

// ── NASA handlers ─────────────────────────────────────────────────────────────

async fn get_apod(
    State(s): State<AppState>,
    _claims: AuthClaims,
) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "apod", Duration::from_secs(60), || async {
        let val = lock_db(&s.db).await.get_apod_latest()?;
        info!("api/apod: served from db");
        Ok(val)
    })
    .await
}

async fn get_neo(
    State(s): State<AppState>,
    _claims: AuthClaims,
) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "neo", Duration::from_secs(60), || async {
        let val = lock_db(&s.db).await.get_neo_recent()?;
        info!("api/neo: served from db");
        Ok(val)
    })
    .await
}

async fn get_epic(
    State(s): State<AppState>,
    _claims: AuthClaims,
) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "epic", Duration::from_secs(60), || async {
        let val = lock_db(&s.db).await.get_epic_latest()?;
        info!("api/epic: served from db");
        Ok(val)
    })
    .await
}

async fn get_exoplanets(
    State(s): State<AppState>,
    _claims: AuthClaims,
) -> Result<impl IntoResponse, AppError> {
    cached(
        &s.cache,
        "exoplanets",
        Duration::from_secs(3600),
        || async {
            let val = lock_db(&s.db).await.get_exoplanets_all()?;
            info!("api/exoplanets: served from db");
            Ok(val)
        },
    )
    .await
}

// ── NOAA handlers ─────────────────────────────────────────────────────────────

async fn get_kp(
    State(s): State<AppState>,
    _claims: AuthClaims,
) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "kp", Duration::from_secs(10), || async {
        let val = lock_db(&s.db).await.get_kp_recent()?;
        info!("api/kp: served from db");
        Ok(val)
    })
    .await
}

async fn get_kp_3h(
    State(s): State<AppState>,
    _claims: AuthClaims,
) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "kp-3h", Duration::from_secs(300), || async {
        let val = lock_db(&s.db).await.get_kp_3h_recent()?;
        info!("api/kp-3h: served from db");
        Ok(val)
    })
    .await
}

async fn get_solar_wind(
    State(s): State<AppState>,
    _claims: AuthClaims,
) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "solar-wind", Duration::from_secs(10), || async {
        let val = lock_db(&s.db).await.get_solar_wind_recent()?;
        info!("api/solar-wind: served from db");
        Ok(val)
    })
    .await
}

async fn get_xray(
    State(s): State<AppState>,
    _claims: AuthClaims,
) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "xray", Duration::from_secs(30), || async {
        let val = lock_db(&s.db).await.get_xray_recent()?;
        info!("api/xray: served from db");
        Ok(val)
    })
    .await
}

async fn get_alerts(
    State(s): State<AppState>,
    _claims: AuthClaims,
) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "alerts", Duration::from_secs(60), || async {
        let val = lock_db(&s.db).await.get_alerts_recent()?;
        info!("api/alerts: served from db");
        Ok(val)
    })
    .await
}

// ── ISS handler ───────────────────────────────────────────────────────────────

async fn get_iss(
    State(s): State<AppState>,
    _claims: AuthClaims,
) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "iss", Duration::from_secs(3), || async {
        let val = lock_db(&s.db).await.get_iss_latest()?;
        info!("api/iss: served from db");
        Ok(val)
    })
    .await
}

// ── ML forecast handler ───────────────────────────────────────────────────────

async fn call_ml_or_cached(s: &AppState) -> Result<serde_json::Value, AppError> {
    let readings = lock_db(&s.db).await.get_recent_kp(7)?;
    if readings.is_empty() {
        return Err(anyhow!("no Kp data in database — poller initializing").into());
    }

    let ml_timeout = std::env::var("ML_TIMEOUT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5u64);

    let result = s
        .client
        .post(format!("{}/predict", s.ml_url))
        .timeout(Duration::from_secs(ml_timeout))
        .json(&serde_json::json!({ "readings": readings }))
        .send()
        .await;

    match result {
        Ok(resp) if resp.status().is_success() => {
            let payload: serde_json::Value = resp.json().await?;
            if let Some(kp) = payload.get("predicted_kp").and_then(|v| v.as_f64()) {
                let forecast_ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64
                    + 3 * 3600;
                let ci_l = payload
                    .get("ci_lower")
                    .and_then(|v| v.as_f64())
                    .map(|v| (v * 100.0).round() as i64);
                let ci_u = payload
                    .get("ci_upper")
                    .and_then(|v| v.as_f64())
                    .map(|v| (v * 100.0).round() as i64);
                let unc = payload
                    .get("uncertainty")
                    .and_then(|v| v.as_f64())
                    .map(|v| (v * 10_000.0).round() as i64);
                s.writer.fire(WriteCmd::KpForecast {
                    ts: forecast_ts,
                    kp_e2: (kp * 100.0).round() as i64,
                    ci_lower_e2: ci_l,
                    ci_upper_e2: ci_u,
                    uncertainty_e4: unc,
                });
            }
            info!("kp-forecast: ML service returned prediction");
            Ok(payload)
        }
        Ok(resp) => {
            let status = resp.status();
            tracing::warn!("kp-forecast: ML service returned {status}, falling back to cache");
            ml_cache_fallback(s).await
        }
        Err(e) => {
            tracing::warn!("kp-forecast: ML service unreachable ({e}), falling back to cache");
            ml_cache_fallback(s).await
        }
    }
}

async fn ml_cache_fallback(s: &AppState) -> Result<serde_json::Value, AppError> {
    match lock_db(&s.db).await.get_kp_forecast_latest()? {
        Some((_, kp_e2)) => Ok(serde_json::json!({
            "predicted_kp": kp_e2 as f64 / 100.0,
            "ci_lower":     serde_json::Value::Null,
            "ci_upper":     serde_json::Value::Null,
            "uncertainty":  serde_json::Value::Null,
            "status":       "degraded",
            "source":       "cache",
        })),
        None => Err(anyhow!("ML service unavailable and no cached forecast").into()),
    }
}

async fn get_kp_forecast(
    State(s): State<AppState>,
    _claims: AuthClaims,
) -> Result<Response, AppError> {
    Ok(cached(
        &s.cache,
        "kp-forecast",
        Duration::from_secs(180),
        || async { call_ml_or_cached(&s).await },
    )
    .await?
    .into_response())
}

fn parse_range(q: &HashMap<String, String>) -> (i64, &'static str) {
    match q.get("range").map(|s| s.as_str()).unwrap_or("7d") {
        "24h" => (24 * 3600, "24h"),
        "30d" => (30 * 86_400, "30d"),
        _ => (7 * 86_400, "7d"),
    }
}

fn now_minus(seconds: i64) -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
        - seconds
}

async fn get_forecast_history(
    State(s): State<AppState>,
    _claims: AuthClaims,
    Query(q): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, AppError> {
    let (range, label) = parse_range(&q);
    let key: &'static str = match label {
        "24h" => "forecast-history-24h",
        "30d" => "forecast-history-30d",
        _ => "forecast-history-7d",
    };
    cached(&s.cache, key, Duration::from_secs(60), || async {
        let val = lock_db(&s.db)
            .await
            .get_forecast_history(now_minus(range))?;
        info!("api/forecast/history: served from db");
        Ok(val)
    })
    .await
}

async fn get_events(
    State(s): State<AppState>,
    _claims: AuthClaims,
    Query(q): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, AppError> {
    let (range, _label) = parse_range(&q);
    let since = now_minus(range);
    let type_filter = q.get("type").map(String::as_str).filter(|s| !s.is_empty());
    let severity_filter = q
        .get("severity")
        .map(String::as_str)
        .filter(|s| !s.is_empty());
    let page: i64 = q
        .get("page")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
        .max(1);
    let page_size: i64 = q
        .get("page_size")
        .and_then(|v| v.parse().ok())
        .unwrap_or(25)
        .clamp(1, 100);

    let val = lock_db(&s.db).await.get_events_page(
        since,
        type_filter,
        severity_filter,
        page,
        page_size,
    )?;
    info!("api/events: served from db");
    Ok(Json(val))
}

async fn get_forecast_metrics(
    State(s): State<AppState>,
    _claims: AuthClaims,
    Query(q): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, AppError> {
    let (range, label) = parse_range(&q);
    let key: &'static str = match label {
        "24h" => "forecast-metrics-24h",
        "30d" => "forecast-metrics-30d",
        _ => "forecast-metrics-7d",
    };
    cached(&s.cache, key, Duration::from_secs(300), || async {
        let val = lock_db(&s.db)
            .await
            .get_forecast_metrics(now_minus(range))?;
        info!("api/forecast/metrics: served from db");
        Ok(val)
    })
    .await
}

// ── IMF / Dst handlers ────────────────────────────────────────────────────────

async fn get_imf(
    State(s): State<AppState>,
    _claims: AuthClaims,
) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "imf", Duration::from_secs(30), || async {
        let val = lock_db(&s.db).await.get_imf_recent()?;
        info!("api/imf: served from db");
        Ok(val)
    })
    .await
}

async fn get_dst(
    State(s): State<AppState>,
    _claims: AuthClaims,
) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "dst", Duration::from_secs(60), || async {
        let val = lock_db(&s.db).await.get_dst_recent()?;
        info!("api/dst: served from db");
        Ok(val)
    })
    .await
}

// ── Starlink handler ──────────────────────────────────────────────────────────

async fn get_starlink(
    State(s): State<AppState>,
    _claims: AuthClaims,
) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "starlink", Duration::from_secs(1800), || async {
        let val = lock_db(&s.db).await.get_starlink_all()?;
        info!(
            "api/starlink: {} satellites served from db",
            val.as_array().map_or(0, |a| a.len())
        );
        Ok(val)
    })
    .await
}

// ── Reports ───────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ReportQuery {
    range: Option<String>,
}

fn range_to_secs(r: &str) -> i64 {
    match r {
        "7d" => 604_800,
        "30d" => 2_592_000,
        _ => 86_400,
    }
}

async fn get_report_summary(
    State(s): State<AppState>,
    _claims: AuthClaims,
    Query(q): Query<ReportQuery>,
) -> Result<impl IntoResponse, AppError> {
    let secs = range_to_secs(q.range.as_deref().unwrap_or("24h"));
    let val = lock_db(&s.db).await.get_report_summary(secs)?;
    info!("api/reports/summary: range={}s", secs);
    Ok(Json(val))
}

async fn get_report_export(
    State(s): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<ReportQuery>,
) -> Result<Response, AppError> {
    if let Some(r) = plan_gate(&s, &claims.sub, "developer").await {
        return Ok(r);
    }
    let secs = range_to_secs(q.range.as_deref().unwrap_or("24h"));
    let csv = lock_db(&s.db).await.get_report_csv(secs)?;
    info!("api/reports/export: range={}s, {} bytes", secs, csv.len());
    let mut res = csv.into_response();
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    res.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"astraeus-report.csv\""),
    );
    Ok(res)
}

async fn get_report_kp(
    State(s): State<AppState>,
    _claims: AuthClaims,
    Query(q): Query<ReportQuery>,
) -> Result<impl IntoResponse, AppError> {
    let secs = range_to_secs(q.range.as_deref().unwrap_or("24h"));
    let val = lock_db(&s.db).await.get_kp_range(secs)?;
    info!(
        "api/reports/kp: range={}s, {} buckets",
        secs,
        val.as_array().map_or(0, |a| a.len())
    );
    Ok(Json(val))
}

async fn get_report_solar_wind(
    State(s): State<AppState>,
    _claims: AuthClaims,
    Query(q): Query<ReportQuery>,
) -> Result<impl IntoResponse, AppError> {
    let secs = range_to_secs(q.range.as_deref().unwrap_or("24h"));
    let val = lock_db(&s.db).await.get_solar_wind_range(secs)?;
    info!(
        "api/reports/solar-wind: range={}s, {} buckets",
        secs,
        val.as_array().map_or(0, |a| a.len())
    );
    Ok(Json(val))
}

// ── Public handlers (no auth) ─────────────────────────────────────────────────

async fn get_public_kp(State(s): State<AppState>) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "pub-kp", Duration::from_secs(10), || async {
        let val = lock_db(&s.db).await.get_kp_array_public()?;
        Ok(val)
    })
    .await
}

async fn get_public_solar_wind(State(s): State<AppState>) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "pub-wind", Duration::from_secs(10), || async {
        let val = lock_db(&s.db).await.get_solar_wind_latest_public()?;
        Ok(val)
    })
    .await
}

async fn get_public_forecast(State(s): State<AppState>) -> Result<impl IntoResponse, AppError> {
    // Shares cache key with /api/kp-forecast — no duplicate ML calls.
    cached(
        &s.cache,
        "kp-forecast",
        Duration::from_secs(180),
        || async { call_ml_or_cached(&s).await },
    )
    .await
}

// ── User handler ──────────────────────────────────────────────────────────────

async fn get_user_me(
    State(s): State<AppState>,
    claims: AuthClaims,
) -> Result<impl IntoResponse, AppError> {
    let val = lock_db(&s.db).await.get_user_me(&claims.sub)?;
    Ok(Json(val))
}

#[derive(serde::Deserialize)]
struct UpdatePlanBody {
    plan: String,
}

async fn update_user_plan(
    State(s): State<AppState>,
    claims: AuthClaims,
    Json(body): Json<UpdatePlanBody>,
) -> Response {
    const VALID_PLANS: &[&str] = &[
        "free",
        "starter",
        "developer",
        "pro",
        "business",
        "enterprise",
    ];
    if !VALID_PLANS.contains(&body.plan.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "invalid plan" })),
        )
            .into_response();
    }
    match s
        .writer
        .update_user_plan(claims.sub.clone(), body.plan)
        .await
    {
        Ok(()) => {
            s.usage_counter.remove(&claims.sub);
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => {
            tracing::error!("update_user_plan: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    }
}

// ── Anomaly handler ───────────────────────────────────────────────────────────

async fn get_anomalies(
    State(s): State<AppState>,
    _claims: AuthClaims,
) -> Result<Response, AppError> {
    Ok(
        cached(&s.cache, "anomalies", Duration::from_secs(30), || async {
            let val = lock_db(&s.db).await.get_anomalies_recent()?;
            info!("api/anomalies: served from db");
            Ok(val)
        })
        .await?
        .into_response(),
    )
}

// ── Usage handler ─────────────────────────────────────────────────────────────

async fn get_usage(
    State(s): State<AppState>,
    claims: AuthClaims,
) -> Result<impl IntoResponse, AppError> {
    let email = &claims.sub;

    // Live count from the in-memory counter (already incremented for this request).
    let live = s
        .usage_counter
        .get(email.as_str())
        .map(|e| (e.count, e.period_start));

    let now_ts = chrono::Utc::now().timestamp();
    let db = lock_db(&s.db).await;
    let plan = db.get_user_plan(email)?;

    let (count, p_start) = if let Some(l) = live {
        l
    } else {
        // Fall back to last flushed DB record (e.g. counter was restarted).
        match db.get_usage_for_user(email)? {
            Some((cnt, ps, _pe)) => (cnt as u64, ps),
            None => (0, crate::rate_limit::current_period_start(&plan, now_ts)),
        }
    };
    let p_end = crate::rate_limit::period_end(&plan, p_start);
    let limit = crate::rate_limit::plan_limit(&plan);

    Ok(Json(serde_json::json!({
        "email":        email,
        "plan":         plan,
        "scope":        "api_key",
        "caller":       if claims.auth_type == AuthType::ApiKey { "api_key" } else { "jwt" },
        "period_start": p_start,
        "period_end":   p_end,
        "count":        count,
        "limit":        limit,
    })))
}

// ── Custom anomaly rules ──────────────────────────────────────────────────────

const VALID_METRICS: &[&str] = &["kp", "solar_wind_speed", "xray_flux", "dst", "imf_bz"];
const VALID_OPERATORS: &[&str] = &["gt", "lt", "gte", "lte"];
const VALID_SEVERITIES: &[&str] = &["warning", "critical"];
const MAX_CUSTOM_RULES: i64 = 20;

#[derive(serde::Deserialize)]
struct CreateCustomRuleBody {
    name: String,
    metric: String,
    operator: String,
    threshold: f64,
    severity: String,
}

#[derive(serde::Deserialize)]
struct ToggleBody {
    enabled: bool,
}

async fn list_custom_rules(
    State(s): State<AppState>,
    claims: AuthClaims,
) -> Result<impl IntoResponse, AppError> {
    let rules = lock_db(&s.db).await.list_custom_rules(&claims.sub)?;
    let json: Vec<_> = rules
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id":         r.id,
                "name":       r.name,
                "metric":     r.metric,
                "operator":   r.operator,
                "threshold":  r.threshold,
                "severity":   r.severity,
                "enabled":    r.enabled,
                "created_at": r.created_at,
            })
        })
        .collect();
    Ok(Json(serde_json::Value::Array(json)))
}

async fn create_custom_rule(
    State(s): State<AppState>,
    claims: AuthClaims,
    Json(body): Json<CreateCustomRuleBody>,
) -> Response {
    if let Some(r) = plan_gate(&s, &claims.sub, "enterprise").await {
        return r;
    }

    // Validate inputs
    let name = body.name.trim().to_string();
    if name.is_empty() || name.len() > 80 {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": "name must be 1–80 characters" })),
        )
            .into_response();
    }
    if !VALID_METRICS.contains(&body.metric.as_str()) {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": "invalid metric" })),
        )
            .into_response();
    }
    if !VALID_OPERATORS.contains(&body.operator.as_str()) {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": "invalid operator" })),
        )
            .into_response();
    }
    if !VALID_SEVERITIES.contains(&body.severity.as_str()) {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": "invalid severity" })),
        )
            .into_response();
    }
    if !body.threshold.is_finite() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": "threshold must be a finite number" })),
        )
            .into_response();
    }

    // Enforce per-user rule cap
    let count = match lock_db(&s.db)
        .await
        .count_custom_rules_for_user(&claims.sub)
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("count_custom_rules: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
    };
    if count >= MAX_CUSTOM_RULES {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "rule limit reached (max 20)" })),
        )
            .into_response();
    }

    let id = format!("{:x}", rand::random::<u64>());
    let now_ts = chrono::Utc::now().timestamp();
    let rule = crate::db::CustomRule {
        id: id.clone(),
        user_email: claims.sub.clone(),
        name: name.clone(),
        metric: body.metric.clone(),
        operator: body.operator.clone(),
        threshold: body.threshold,
        severity: body.severity.clone(),
        enabled: true,
        created_at: now_ts,
    };

    match s.writer.create_custom_rule(rule).await {
        Ok(()) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "id":        id,
                "name":      name,
                "metric":    body.metric,
                "operator":  body.operator,
                "threshold": body.threshold,
                "severity":  body.severity,
                "enabled":   true,
                "created_at": now_ts,
            })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("create_custom_rule: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response()
        }
    }
}

async fn delete_custom_rule(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<String>,
) -> Response {
    match s.writer.delete_custom_rule(id, claims.sub).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "rule not found" })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("delete_custom_rule: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response()
        }
    }
}

async fn toggle_custom_rule(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<String>,
    Json(body): Json<ToggleBody>,
) -> Response {
    match s
        .writer
        .toggle_custom_rule(id, claims.sub, body.enabled)
        .await
    {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "rule not found" })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("toggle_custom_rule: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response()
        }
    }
}

// ── MCP (Model Context Protocol) ──────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct McpRequest {
    id: Option<serde_json::Value>,
    method: String,
    params: Option<serde_json::Value>,
}

#[derive(serde::Serialize)]
struct McpResp {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<serde_json::Value>,
}

impl McpResp {
    fn ok(id: Option<serde_json::Value>, result: serde_json::Value) -> Response {
        Json(Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        })
        .into_response()
    }
    fn err(id: Option<serde_json::Value>, code: i32, msg: &str) -> Response {
        Json(Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(serde_json::json!({ "code": code, "message": msg })),
        })
        .into_response()
    }
}

fn mcp_text(data: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "content": [{ "type": "text", "text": data.to_string() }] })
}

const MCP_TOOLS: &str = r#"{"tools":[
  {"name":"get_current_kp","description":"Get the current Kp index and recent readings from NOAA (no auth required).","inputSchema":{"type":"object","properties":{}}},
  {"name":"get_solar_wind","description":"Get the latest solar wind speed and density from NOAA DSCOVR (no auth required).","inputSchema":{"type":"object","properties":{}}},
  {"name":"get_kp_forecast","description":"Get the ML 3-hour Kp forecast with 95% confidence interval (no auth required).","inputSchema":{"type":"object","properties":{}}},
  {"name":"get_health","description":"Get service health status for all data sources (no auth required).","inputSchema":{"type":"object","properties":{}}},
  {"name":"get_anomalies","description":"Get detected space weather anomalies: storms, flares, solar wind spikes, asteroid close approaches. Requires Bearer token.","inputSchema":{"type":"object","properties":{}}},
  {"name":"get_neo","description":"Get NASA near-Earth object close approaches for the next 7 days with hazard flags. Requires Bearer token.","inputSchema":{"type":"object","properties":{}}},
  {"name":"get_iss_position","description":"Get current ISS position, altitude, and velocity. Requires Bearer token.","inputSchema":{"type":"object","properties":{}}}
]}"#;

async fn mcp_handler(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<McpRequest>,
) -> Response {
    // Notifications have no id and require no response body.
    if req.method.starts_with("notifications/") {
        return StatusCode::NO_CONTENT.into_response();
    }

    let id = req.id.clone();

    match req.method.as_str() {
        "initialize" => McpResp::ok(
            id,
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "serverInfo": { "name": "Astraeusio Space Weather", "version": "1.0.0" },
                "capabilities": { "tools": {} }
            }),
        ),

        "tools/list" => {
            let tools: serde_json::Value = serde_json::from_str(MCP_TOOLS).unwrap();
            McpResp::ok(id, tools)
        }

        "tools/call" => {
            let name = req
                .params
                .as_ref()
                .and_then(|p| p.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("");

            // Extract JWT from Authorization header.
            let token_opt = headers
                .get("Authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "));

            match name {
                "get_current_kp" => match lock_db(&s.db).await.get_kp_array_public() {
                    Ok(v) => McpResp::ok(id, mcp_text(v)),
                    Err(e) => McpResp::err(id, -32603, &e.to_string()),
                },
                "get_solar_wind" => match lock_db(&s.db).await.get_solar_wind_latest_public() {
                    Ok(v) => McpResp::ok(id, mcp_text(v)),
                    Err(e) => McpResp::err(id, -32603, &e.to_string()),
                },
                "get_kp_forecast" => match call_ml_or_cached(&s).await {
                    Ok(v) => McpResp::ok(id, mcp_text(v)),
                    Err(e) => McpResp::err(id, -32603, &format!("{}", e.0)),
                },
                "get_health" => {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;
                    let (noaa_ts, nasa_ts, _) = lock_db(&s.db).await.health_freshness();
                    let noaa_ok = noaa_ts.is_some_and(|t| now - t < 600);
                    let nasa_ok = nasa_ts.is_some_and(|t| now - t < 90_000);
                    McpResp::ok(
                        id,
                        mcp_text(serde_json::json!({
                            "status": if noaa_ok && nasa_ok { "operational" } else { "degraded" },
                            "noaa":   if noaa_ok { "operational" } else { "degraded" },
                            "nasa":   if nasa_ok { "operational" } else { "degraded" },
                            "checked_at": now,
                        })),
                    )
                }
                "get_anomalies" | "get_neo" | "get_iss_position" => {
                    let authed = token_opt.is_some_and(|t| {
                        use jsonwebtoken::{DecodingKey, Validation, decode};
                        decode::<serde_json::Value>(
                            t,
                            &DecodingKey::from_secret(s.jwt_secret.as_bytes()),
                            &Validation::default(),
                        )
                        .is_ok()
                    });
                    if !authed {
                        return McpResp::err(
                            id,
                            -32001,
                            "authentication required: provide Authorization: Bearer <token>",
                        );
                    }
                    match name {
                        "get_anomalies" => match lock_db(&s.db).await.get_anomalies_recent() {
                            Ok(v) => McpResp::ok(id, mcp_text(v)),
                            Err(e) => McpResp::err(id, -32603, &e.to_string()),
                        },
                        "get_neo" => match lock_db(&s.db).await.get_neo_recent() {
                            Ok(v) => McpResp::ok(id, mcp_text(v)),
                            Err(e) => McpResp::err(id, -32603, &e.to_string()),
                        },
                        _ => match lock_db(&s.db).await.get_iss_latest() {
                            Ok(v) => McpResp::ok(id, mcp_text(v)),
                            Err(e) => McpResp::err(id, -32603, &e.to_string()),
                        },
                    }
                }
                _ => McpResp::err(id, -32601, &format!("unknown tool: {name}")),
            }
        }

        _ => McpResp::err(id, -32601, &format!("method not found: {}", req.method)),
    }
}
