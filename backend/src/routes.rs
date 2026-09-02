use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, MutexGuard};

use anyhow::anyhow;
use axum::{
    Json, Router,
    extract::{FromRequestParts, Path, Query, State},
    http::{HeaderValue, StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use serde::Deserialize;
use tower_http::cors::{Any, CorsLayer};
use tracing::{info, warn};

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

// Keys are mostly literals, but the forecast history varies by horizon as well
// as by range, so the key has to be built. Cow keeps the literal call sites
// allocation free.
type CacheMap = HashMap<std::borrow::Cow<'static, str>, (Instant, serde_json::Value)>;

async fn cached<F, Fut, K>(
    cache: &Arc<Mutex<CacheMap>>,
    key: K,
    ttl: Duration,
    fetch: F,
) -> Result<Json<serde_json::Value>, AppError>
where
    K: Into<std::borrow::Cow<'static, str>>,
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<serde_json::Value, AppError>>,
{
    let key = key.into();
    {
        let guard = cache.lock().await;
        if let Some((ts, val)) = guard.get(&key)
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
    /// Consecutive failed sign in attempts per account. In memory and per
    /// process, so it resets when the container restarts.
    pub login_failures: Arc<crate::rate_limit::LoginFailures>,
    pub mailer: Option<mailer::MailerConfig>,
    pub app_url: String,
    pub oauth: crate::oauth::OAuthConfig,
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        client: reqwest::Client,
        db: Store,
        writer: DbWriterHandle,
        ml_url: String,
        jwt_secret: String,
        mailer: Option<mailer::MailerConfig>,
        app_url: String,
        oauth: crate::oauth::OAuthConfig,
    ) -> Self {
        Self {
            client,
            db: Arc::new(Mutex::new(db)),
            writer,
            ml_url,
            cache: Arc::new(Mutex::new(HashMap::new())),
            jwt_secret,
            usage_counter: Arc::new(DashMap::new()),
            login_failures: Arc::new(DashMap::new()),
            mailer,
            app_url,
            oauth,
        }
    }
}

// ── Error ─────────────────────────────────────────────────────────────────────

pub struct AppError {
    err: anyhow::Error,
    status: StatusCode,
}

impl AppError {
    fn with_status(err: anyhow::Error, status: StatusCode) -> Self {
        Self { err, status }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        tracing::error!("{}", self.err);
        (
            self.status,
            Json(serde_json::json!({ "error": self.err.to_string() })),
        )
            .into_response()
    }
}

impl<E: Into<anyhow::Error>> From<E> for AppError {
    fn from(e: E) -> Self {
        Self {
            err: e.into(),
            status: StatusCode::INTERNAL_SERVER_ERROR,
        }
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

/// Refuses an action to an account whose address is unproven.
///
/// Verification gates writing and spending, not reading and not signing in. An
/// unverified address is an unproven claim to an identity, so it should not
/// accumulate credentials, and it should not cause us to send mail to it. It is
/// not a reason to withhold data the account can already see, and refusing sign
/// in turns a soft problem into one only support can undo.
///
/// So this sits on: creating an API key, creating a webhook, creating a custom
/// rule, setting email alert thresholds, and changing plan. Deletion is
/// deliberately not gated, because taking a credential away is the safe
/// direction and an account should always be able to reduce its own exposure.
///
/// The response names the address and the way out, because a 403 whose remedy
/// the reader has to guess is how an account becomes a support ticket.
pub(crate) async fn verified_gate(s: &AppState, email: &str) -> Option<Response> {
    let verified = match lock_db(&s.db).await.find_user_by_email(email) {
        Ok(Some(u)) => u.email_verified,
        // No row and a valid token should not co-occur. Refusing is the safe
        // reading of a state that should not exist.
        Ok(None) => false,
        Err(e) => {
            warn!("verified_gate lookup error: {e}");
            return Some(
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "internal error" })),
                )
                    .into_response(),
            );
        }
    };
    if verified {
        return None;
    }
    Some(
        (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error":  "email_verification_required",
                "email":  email,
                "detail": "Confirm your email address to use this. \
                           Settings has a button to send the link again.",
                "resend": "/auth/resend-verification",
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
    let (series, liveness, celestrak_ts) = {
        let guard = lock_db(&s.db).await;
        let celestrak = guard.external_freshness();
        // Two kinds of component, and the difference is which question can be
        // asked. A series is judged on the age of its newest row. A feed that
        // publishes only when something happens is judged on the verdict its
        // poller recorded, because for that one no row age separates quiet from
        // dead.
        (guard.series_health(), guard.poll_liveness(), celestrak)
    };

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

    let (celestrak_status, celestrak_last) = component_status(celestrak_ts, now, 14_400);
    // The database is reachable if any series has ever stored a reading. That
    // is separate from whether the feeds are still arriving, which is what the
    // per-series components report.
    let db_last = series.iter().filter_map(|(_, _, ts)| *ts).max();
    let db_status = if db_last.is_some() {
        "operational"
    } else {
        "unknown"
    };

    // ── Overall ───────────────────────────────────────────────────────────────
    //
    // Every component is still published below. What `status` answers is
    // narrower: does the product work. The NASA auxiliary feeds are excluded
    // (`db::AUXILIARY`), because an astronomy picture failing to fetch was
    // putting "degraded" on a public page that people read as a statement about
    // space weather data. Contract change, 2026-09-01, and the reason it is
    // here rather than in the page is that a cron mail contradicting the public
    // status page is worse than a changed field.
    let overall = if [ml_status, db_status, celestrak_status]
        .iter()
        .all(|&s| s == "operational")
        && crate::db::all_product_components_operational(&series)
        && crate::db::all_product_components_operational(&liveness)
    {
        "operational"
    } else {
        "degraded"
    };

    let mut components = serde_json::Map::new();
    components.insert(
        "backend_api".into(),
        serde_json::json!({ "status": "operational", "last_checked": now }),
    );
    components.insert(
        "ml_forecast".into(),
        serde_json::json!({ "status": ml_status, "last_checked": now }),
    );
    components.insert(
        "database".into(),
        serde_json::json!({ "status": db_status, "last_write": db_last }),
    );
    for (component, status, last) in &series {
        components.insert(
            (*component).into(),
            serde_json::json!({ "status": status, "last_update": last }),
        );
    }
    // `last_update` is the time of the verdict, not of the newest alert. That
    // is the honest label for it: the field says when we last checked, which is
    // what a reader of this component needs to know.
    for (component, status, last) in &liveness {
        components.insert(
            (*component).into(),
            serde_json::json!({ "status": status, "last_update": last }),
        );
    }
    components.insert(
        "celestrak".into(),
        serde_json::json!({ "status": celestrak_status, "last_update": celestrak_last }),
    );

    Json(serde_json::json!({
        "status":     overall,
        "checked_at": now,
        "components": components
    }))
}

/// Ninety days of per-component uptime.
///
/// The percentage is operational samples over samples **due**, not over samples
/// present. Under the old denominator an outage cancelled itself out: a backend
/// that is down writes no health rows, so the missing samples left both halves
/// of the fraction and the figure stayed at 100. Counting what should have been
/// written makes absence the evidence it always was.
///
/// Expectation starts at a component's first ever sample, never before, which
/// is what keeps `2623cf6` intact: a component added yesterday still reads as
/// ninety days of no data rather than eighty-nine days of outage.
async fn uptime(State(s): State<AppState>) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "uptime_90d", Duration::from_secs(300), || async {
        const DAYS: i64 = 90;
        let interval = crate::poller::health_interval_secs().max(1) as i64;
        let now = chrono::Utc::now().timestamp();
        let today = now / 86_400;
        let (rows, first_seen) = {
            let db = lock_db(&s.db).await;
            (db.uptime_by_day(DAYS)?, db.health_first_sample()?)
        };
        // rows: (component, utc_day, samples_present, operational_samples)
        // "nasa" is deliberately absent: it was split into one component per
        // feed, which now come from SERIES_FRESHNESS below. Its historical rows
        // stay in health_snapshots and simply stop being rendered.
        let fixed = ["backend_api", "ml_forecast", "database", "celestrak"];
        // `POLL_LIVENESS` joins `SERIES_FRESHNESS` here because a component
        // whose status is published and whose history is not is half published.
        // `noaa_alerts` showed on the status page with an empty strip from the
        // day it was added until 2026-09-01.
        let components = fixed
            .into_iter()
            .chain(crate::db::SERIES_FRESHNESS.iter().map(|s| s.component))
            .chain(crate::db::POLL_LIVENESS.iter().map(|l| l.component));

        /// Samples due for one component on one UTC day: the part of that day
        /// that lies after its first sample and before now, divided by the
        /// interval. A day before the component existed expects nothing, and so
        /// does a day that has not happened yet.
        fn samples_due(day: i64, first: i64, now: i64, interval: i64) -> i64 {
            let day_start = day * 86_400;
            let from = day_start.max(first);
            let to = (day_start + 86_400).min(now);
            if to <= from { 0 } else { (to - from) / interval }
        }

        let mut out = serde_json::Map::new();
        for comp in components {
            let liveness = crate::db::LIVENESS_ONLY.contains(&comp);
            let first = first_seen.iter().find(|(c, _)| c == comp).map(|(_, t)| *t);
            // 90 entries, oldest first (index 0 = 89 days ago, last = today)
            let mut days: Vec<serde_json::Value> = (0..DAYS)
                .map(|_| serde_json::json!({"status": "no_data", "uptime_pct": null}))
                .collect();
            let mut total_due = 0i64;
            let mut total_ok = 0i64;
            let mut recorded_days = 0i64;

            for idx in 0..DAYS {
                let day = today - (DAYS - 1 - idx);
                let Some(first) = first else { continue };
                let due = samples_due(day, first, now, interval);
                if due <= 0 {
                    continue;
                }
                let (present, operational) = rows
                    .iter()
                    .find(|(c, d, _, _)| c == comp && *d == day)
                    .map(|(_, _, p, o)| (*p, *o))
                    .unwrap_or((0, 0));
                // A liveness component has no verdict, so the sample being
                // there is the whole of what it can say.
                let ok = if liveness { present } else { operational };
                // Restarts and interval jitter can write more samples than the
                // arithmetic expects. More than complete is still complete.
                let pct = ((ok as f64 / due as f64) * 100.0).min(100.0);
                let status = if pct >= 99.0 {
                    "operational"
                } else if pct >= 90.0 {
                    "degraded"
                } else {
                    "outage"
                };
                days[idx as usize] = serde_json::json!({
                    "status": status,
                    "uptime_pct": (pct * 100.0).round() / 100.0,
                });
                total_due += due;
                total_ok += ok.min(due);
                recorded_days += 1;
            }

            // Null, not zero, when nothing was ever recorded. A component added
            // yesterday has no history, and reporting 0 percent would read as
            // three months of downtime. The percentage covers only the days
            // actually observed, and recorded_days says how many that is, so the
            // figure can be labelled with the window it really describes.
            let overall = if total_due > 0 {
                serde_json::json!(
                    ((total_ok as f64 / total_due as f64) * 10_000.0).round() / 100.0
                )
            } else {
                serde_json::Value::Null
            };
            out.insert(
                comp.to_string(),
                serde_json::json!({
                    "uptime_pct":    overall,
                    "recorded_days": recorded_days,
                    "measures":      if liveness { "liveness" } else { "health" },
                    "days":          days,
                }),
            );
        }
        Ok(serde_json::json!({
            "window_days": DAYS,
            "components":  out,
        }))
    })
    .await
}


pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/health", get(health))
        .route("/api/health/uptime", get(uptime))
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
        .route("/auth/oauth/{provider}/start", get(crate::oauth::start))
        .route(
            "/auth/oauth/{provider}/callback",
            get(crate::oauth::callback),
        )
        .route("/api/auth/providers", get(crate::oauth::list_providers))
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
        .route("/api/astros", get(get_astros))
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
        .route("/api/webhooks/{id}/deliveries", get(webhooks::list_deliveries))
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

async fn get_astros(
    State(s): State<AppState>,
    _claims: AuthClaims,
) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "astros", Duration::from_secs(21_600), || async {
        let summary = crate::astros::fetch_astros(&s.client)
            .await
            .map_err(|e| anyhow!("astros fetch failed: {e}"))?;
        info!("api/astros: fetched live from LL2");
        Ok(serde_json::to_value(summary)?)
    })
    .await
}

// ── ML forecast handler ───────────────────────────────────────────────────────

/// Asks the ML service how long an input sequence its checkpoint expects.
///
/// Read from the service rather than duplicated as a backend constant, so a
/// retrained model with a different lookback does not silently get fed the
/// wrong number of periods.
pub(crate) async fn ml_seq_len(client: &reqwest::Client, ml_url: &str) -> anyhow::Result<usize> {
    let health: serde_json::Value = client
        .get(format!("{ml_url}/health"))
        .timeout(Duration::from_secs(3))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    health
        .get("seq_len")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .ok_or_else(|| anyhow!("ML service did not report seq_len"))
}

async fn call_ml_or_cached(s: &AppState) -> Result<serde_json::Value, AppError> {
    let seq_len = ml_seq_len(&s.client, &s.ml_url).await.map_err(|e| {
        AppError::with_status(
            anyhow!("ML service unavailable: {e}"),
            StatusCode::SERVICE_UNAVAILABLE,
        )
    })?;

    // The model is trained on the three-hour series, so a short window is a
    // hard failure: the ML service would otherwise be handed a sequence it
    // cannot use. A stale window fails the same way, because a forecast built
    // from readings that stopped weeks ago still reads as current.
    let readings = lock_db(&s.db)
        .await
        .get_recent_kp_3h(seq_len)
        .map_err(|e| match e {
            crate::db::DbError::InsufficientHistory { .. } => AppError::with_status(
                anyhow!("{e}"),
                StatusCode::SERVICE_UNAVAILABLE,
            ),
            crate::db::DbError::StaleSeries {
                series,
                newest_observed_at,
            } => {
                warn!(
                    source = "api/kp-forecast",
                    series, newest_observed_at, "refusing to forecast from a stale input series"
                );
                AppError::with_status(anyhow!("{e}"), StatusCode::SERVICE_UNAVAILABLE)
            }
            other => other.into(),
        })?;

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
            // Same parser as the poller. Two call sites reading `forecast[]`
            // their own way is how three of four horizons would go missing on
            // one path and not the other.
            match crate::db::ForecastPoint::from_predict_payload(&payload) {
                Ok((points, model_sha)) => {
                    let issued_at = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;
                    s.writer.fire(WriteCmd::KpForecast {
                        issued_at,
                        model_sha,
                        points,
                    });
                }
                // Served to the caller, stored for nobody. The response is
                // still useful to read; it is not useful to score against.
                Err(e) => tracing::warn!(source = "api/kp-forecast", "{e}"),
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
    // The 3 h head, matching the flat fields this response mirrors.
    match lock_db(&s.db).await.get_kp_forecast_latest(3)? {
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
    // Defaults to the 3 h series, which is what the page charted when one
    // horizon was all there was. An unrecognised value is the default rather
    // than an error, the same way the range parameter behaves.
    let horizon = q
        .get("horizon")
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|h| crate::db::FORECAST_HORIZONS.contains(h))
        .unwrap_or(3);
    let key: String = format!("forecast-history-{label}-{horizon}");
    cached(&s.cache, key, Duration::from_secs(60), || async {
        let val = lock_db(&s.db)
            .await
            .get_forecast_history(now_minus(range), horizon)?;
        info!(horizon, "api/forecast/history: served from db");
        Ok(val)
    })
    .await
}

async fn get_events(
    State(s): State<AppState>,
    claims: AuthClaims,
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
        &claims.sub,
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
    claims: AuthClaims,
    Query(q): Query<ReportQuery>,
) -> Result<impl IntoResponse, AppError> {
    let secs = range_to_secs(q.range.as_deref().unwrap_or("24h"));
    let val = lock_db(&s.db).await.get_report_summary(&claims.sub, secs)?;
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
    // Shares cache key with /api/kp-forecast - no duplicate ML calls.
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

pub(crate) const VALID_PLANS: &[&str] = &[
    "free",
    "developer",
    "pro",
    "business",
    "enterprise",
];

/// Whether a signed in account may set its own tier.
///
/// Off unless `ALLOW_SELF_SERVE_PLAN_CHANGE` is `1` or `true`. No payment
/// processor is connected, so nothing in the system can tell a paid tier from
/// an unpaid one, and the endpoint validated only that the string was a known
/// plan name. Any account could therefore grant itself enterprise and with it
/// unlimited quota, CSV export, API keys, email alerts, webhooks and custom
/// rules. The flag keeps the endpoint usable in development and closes it
/// everywhere the variable is unset, which includes production.
pub(crate) fn self_serve_plan_change_enabled() -> bool {
    matches!(
        std::env::var("ALLOW_SELF_SERVE_PLAN_CHANGE").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("True")
    )
}

/// Moves an account to a tier and clears the cached counter so the new tier
/// applies to the next request.
///
/// This is the whole mutation. When billing is connected, the payment webhook
/// calls this and becomes the only caller that may raise a tier; the handler
/// below and its environment flag are then deleted, and nothing else moves.
async fn apply_plan_change(s: &AppState, email: &str, plan: String) -> Result<(), crate::db::DbError> {
    s.writer.update_user_plan(email.to_string(), plan).await?;
    crate::rate_limit::clear_user_cache(&s.usage_counter, email);
    Ok(())
}

async fn update_user_plan(
    State(s): State<AppState>,
    claims: AuthClaims,
    Json(body): Json<UpdatePlanBody>,
) -> Response {
    if let Some(r) = verified_gate(&s, &claims.sub).await {
        return r;
    }
    if !VALID_PLANS.contains(&body.plan.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "invalid plan" })),
        )
            .into_response();
    }
    // Moving down a tier gives nothing away, so it stays self serve. Only a
    // raise needs the flag, because with no payment processor connected nothing
    // in the system can tell a paid tier from an unpaid one.
    let current = plan::resolve(&s.usage_counter, &s.db, &claims.sub).await;
    let is_raise = plan::rank(&body.plan) > plan::rank(&current);
    if is_raise && !self_serve_plan_change_enabled() {
        warn!(
            source = "api/user/plan",
            subject = %claims.sub,
            from = %current,
            requested = %body.plan,
            "self serve upgrade is disabled, refusing"
        );
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "plan_upgrade_unavailable",
                "message": "Plans cannot be upgraded here. Contact sales to move to a paid plan.",
            })),
        )
            .into_response();
    }
    match apply_plan_change(&s, &claims.sub, body.plan).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
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

/// Not cached. The response is now per account, and the response cache is keyed
/// on a single static string, so caching here would hand one account's custom
/// rule anomalies to whoever asked next. That is the same disclosure the
/// user_email column exists to stop. The query is bounded to 150 rows against a
/// small table, so the cost of reading it each time is slight.
async fn get_anomalies(
    State(s): State<AppState>,
    claims: AuthClaims,
) -> Result<Response, AppError> {
    let val = lock_db(&s.db).await.get_anomalies_recent(&claims.sub)?;
    info!(source = "api/anomalies", "served from db");
    Ok(Json(val).into_response())
}

// ── Usage handler ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct UsageQuery {
    /// Optional. Absent means the period in progress. A period with no stored
    /// row reports zero rather than failing, because no row and no usage are
    /// the same thing.
    period_start: Option<i64>,
}

async fn get_usage(
    State(s): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<UsageQuery>,
) -> Result<impl IntoResponse, AppError> {
    let email = &claims.sub;
    let now_ts = chrono::Utc::now().timestamp();
    let db = lock_db(&s.db).await;
    let plan = db.get_user_plan(email)?;
    let current_start = crate::rate_limit::current_period_start(&plan, now_ts);

    let (count, p_start) = match q.period_start {
        // A named past period comes from the record alone. Absent means zero.
        Some(requested) => {
            let stored = db.get_usage_for_period(email, requested)?;
            (stored.map(|(c, _, _)| c as u64).unwrap_or(0), requested)
        }
        // The period in progress: the counter is authoritative because the
        // flush only runs every 60 seconds. Quota is charged to API keys only,
        // so a dashboard only account correctly reads zero.
        None => {
            let live = s
                .usage_counter
                .get(email.as_str())
                .filter(|e| e.period_start == current_start)
                .map(|e| e.count);
            match live {
                Some(c) => (c, current_start),
                None => {
                    let stored = db.get_usage_for_period(email, current_start)?;
                    (stored.map(|(c, _, _)| c as u64).unwrap_or(0), current_start)
                }
            }
        }
    };

    let p_end = crate::rate_limit::period_end(&plan, p_start);
    let limit = crate::rate_limit::plan_limit(&plan);
    let history: Vec<serde_json::Value> = db
        .list_usage_history(email, 24)?
        .into_iter()
        .map(|(c, ps, pe)| {
            serde_json::json!({ "period_start": ps, "period_end": pe, "count": c })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "email":        email,
        "plan":         plan,
        // `scope` describes the count, `caller` describes the reader, and they
        // are allowed to disagree. `check_and_increment` runs on one branch of
        // the extractor, the API key branch, so the counter only ever holds API
        // key requests however the figure is fetched. AUD-029 read the two
        // adjacent fields as a contradiction and prescribed deriving `scope`
        // from `auth_type`, which would have a session caller told the count
        // covers session requests. It does not, and that trades a redundant
        // truth for a fresh falsehood. What was actually missing is the line
        // below: nothing said the exclusion was deliberate rather than a gap.
        "scope":                   "api_key",
        "counts_session_requests": false,
        "caller":       if claims.auth_type == AuthType::ApiKey { "api_key" } else { "jwt" },
        "period_start": p_start,
        "period_end":   p_end,
        "count":        count,
        "limit":        limit,
        "history":      history,
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
                "threshold":  crate::anomaly::unscale_threshold(&r.metric, r.threshold_scaled),
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
    if let Some(r) = verified_gate(&s, &claims.sub).await {
        return r;
    }
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
    // The column holds the metric's own scaled integer, so the threshold is
    // converted here and refused if it carries more precision than the metric
    // stores. Silently moving someone's threshold changes when their alert fires
    // and they would never know.
    let threshold_scaled = match crate::anomaly::scale_threshold(&body.metric, body.threshold) {
        Ok(v) => v,
        Err(crate::anomaly::ThresholdError::TooPrecise { step }) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({
                    "error": "threshold_too_precise",
                    "smallest_step": step,
                    "message": format!("This metric is measured in steps of {step}. Round your threshold to that."),
                })),
            )
                .into_response();
        }
        Err(_) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({ "error": "threshold must be a finite number in range" })),
            )
                .into_response();
        }
    };

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
        threshold_scaled,
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
  {"name":"get_kp_forecast","description":"Get the ML 3-hour Kp forecast with the model spread across 50 inference passes (no auth required).","inputSchema":{"type":"object","properties":{}}},
  {"name":"get_health","description":"Get service health status for all data sources (no auth required).","inputSchema":{"type":"object","properties":{}}},
  {"name":"get_anomalies","description":"Get detected space weather anomalies: storms, flares, solar wind spikes, asteroid close approaches. Requires Bearer token.","inputSchema":{"type":"object","properties":{}}},
  {"name":"get_neo","description":"Get NASA near-Earth object close approaches for the next 7 days with hazard flags. Requires Bearer token.","inputSchema":{"type":"object","properties":{}}},
  {"name":"get_iss_position","description":"Get current ISS position, altitude, and velocity. Requires Bearer token.","inputSchema":{"type":"object","properties":{}}}
]}"#;

async fn mcp_handler(
    State(s): State<AppState>,
    mut parts: Parts,
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
                    Err(e) => McpResp::err(id, -32603, &format!("{}", e.err)),
                },
                "get_health" => {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;
                    let series = lock_db(&s.db).await.series_health();
                    // Each rollup covers only its own family, and is degraded if
                    // any one member is, so a live feed cannot cover for a dead
                    // one. Per-series detail sits alongside them. nasa is derived
                    // the same way as noaa now; it used to be a single
                    // MAX(fetched_at) across apod, neo and epic, which the daily
                    // APOD held green while the other two were dead.
                    let group_ok = |prefix: &str| {
                        series
                            .iter()
                            .filter(|(component, _, _)| component.starts_with(prefix))
                            .all(|(_, status, _)| *status == "operational")
                    };
                    let noaa_ok = group_ok("noaa_");
                    let nasa_ok = group_ok("nasa_");
                    let all_ok = series.iter().all(|(_, status, _)| *status == "operational");
                    let per_series: serde_json::Map<String, serde_json::Value> = series
                        .iter()
                        .map(|(component, status, _)| {
                            ((*component).to_string(), serde_json::json!(status))
                        })
                        .collect();
                    McpResp::ok(
                        id,
                        mcp_text(serde_json::json!({
                            "status": if all_ok { "operational" } else { "degraded" },
                            "noaa":   if noaa_ok { "operational" } else { "degraded" },
                            "nasa":   if nasa_ok { "operational" } else { "degraded" },
                            "series": per_series,
                            "checked_at": now,
                        })),
                    )
                }
                "get_anomalies" | "get_neo" | "get_iss_position" => {
                    // One extractor for every authenticated surface. The check
                    // here used to decode into serde_json::Value and accept any
                    // validly signed token, including the OAuth state token that
                    // the unauthenticated start endpoint hands to any caller.
                    // Going through AuthClaims brings the audience check, API key
                    // support and quota counting with it.
                    let claims = match AuthClaims::from_request_parts(&mut parts, &s).await {
                        Ok(c) => c,
                        Err(_) => {
                            return McpResp::err(
                                id,
                                -32001,
                                "authentication required: provide Authorization: Bearer <token>",
                            );
                        }
                    };
                    info!(source = "mcp", tool = name, subject = %claims.sub, "tool call");
                    match name {
                        "get_anomalies" => match lock_db(&s.db).await.get_anomalies_recent(&claims.sub) {
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

#[cfg(test)]
mod mcp_tests {
    use super::*;
    use crate::auth::{TokenPurpose, purpose_token, session_jwt};
    use axum::http::Request;

    const SECRET: &str = "test-secret-not-used-anywhere-real";

    fn test_state() -> AppState {
        let client = reqwest::Client::new();
        AppState::new(
            client.clone(),
            Store::open(":memory:").expect("in-memory store"),
            crate::db_writer::spawn(Store::open(":memory:").expect("writer store"), client),
            "http://ml".to_string(),
            SECRET.to_string(),
            None,
            "http://app".to_string(),
            crate::oauth::OAuthConfig {
                github: None,
                google: None,
                redirect_base: "http://app".to_string(),
            },
        )
    }

    /// Calls the real MCP handler for a tool, with an optional bearer token,
    /// and returns the decoded JSON-RPC response.
    async fn call_tool(state: &AppState, tool: &str, token: Option<&str>) -> serde_json::Value {
        let mut builder = Request::builder();
        if let Some(t) = token {
            builder = builder.header("Authorization", format!("Bearer {t}"));
        }
        let (parts, ()) = builder.body(()).expect("request").into_parts();
        let body = McpRequest {
            id: Some(serde_json::json!(1)),
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({ "name": tool })),
        };
        let resp = mcp_handler(State(state.clone()), parts, Json(body)).await;
        let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
            .await
            .expect("body");
        serde_json::from_slice(&bytes).expect("json")
    }

    /// Calls the uptime handler and decodes it.
    async fn call_uptime(state: &AppState) -> serde_json::Value {
        let Ok(resp) = uptime(State(state.clone())).await else {
            panic!("uptime handler failed");
        };
        let bytes = axum::body::to_bytes(resp.into_response().into_body(), 1 << 20)
            .await
            .expect("body");
        serde_json::from_slice(&bytes).expect("json")
    }

    fn is_auth_error(v: &serde_json::Value) -> bool {
        v.get("error").and_then(|e| e.get("code")).and_then(|c| c.as_i64()) == Some(-32001)
    }

    /// The MCP check used to decode into serde_json::Value and accept any
    /// validly signed token. The OAuth state token is the sharp end: the start
    /// endpoint is unauthenticated and hands it to any caller in a redirect, so
    /// an anonymous attacker could read the whole anomaly feed with it.
    #[tokio::test]
    async fn mcp_rejects_tokens_that_are_not_sessions() {
        let state = test_state();
        let (oauth_state, _) = crate::oauth::sign_state("github", SECRET).expect("mint state");
        let verify = purpose_token("user@example.com", TokenPurpose::VerifyEmail, 300, SECRET, 0)
            .expect("mint verify");
        let partial =
            purpose_token("user@example.com", TokenPurpose::TwoFactorPartial, 300, SECRET, 0)
                .expect("mint partial");

        for (label, token) in [
            ("no header", None),
            ("oauth state token", Some(oauth_state.as_str())),
            ("verify_email token", Some(verify.as_str())),
            ("2fa partial token", Some(partial.as_str())),
            ("garbage", Some("not-a-jwt")),
        ] {
            for tool in ["get_anomalies", "get_neo", "get_iss_position"] {
                let v = call_tool(&state, tool, token).await;
                assert!(
                    is_auth_error(&v),
                    "{tool} must reject {label}, got {v}"
                );
            }
        }
    }

    /// A real session token still reaches the tool. The store is empty, so the
    /// assertion is that the call got past authentication, not what it returned.
    #[tokio::test]
    async fn mcp_accepts_a_session_token() {
        let state = test_state();
        let token = session_jwt("user@example.com", SECRET, 0).expect("mint");
        for tool in ["get_anomalies", "get_neo", "get_iss_position"] {
            let v = call_tool(&state, tool, Some(&token)).await;
            assert!(!is_auth_error(&v), "{tool} must accept a session token, got {v}");
        }
    }

    /// `scope` names what the count covers, `caller` names who asked, and the
    /// two are allowed to disagree. AUD-029 read them as contradicting and
    /// prescribed deriving `scope` from `auth_type`. That is asserted against
    /// here: quota is charged on the API key branch of the extractor alone, so
    /// a session caller told `"scope": "jwt"` would be told the count includes
    /// its own requests, which is a claim nothing in the system makes true.
    ///
    /// The field that closes the finding is `counts_session_requests`, because
    /// the real gap was that a zero looked like a missing figure rather than a
    /// deliberate exclusion.
    #[tokio::test]
    async fn the_usage_scope_describes_the_count_not_the_caller() {
        let state = test_state();
        for auth_type in [AuthType::ApiKey, AuthType::Jwt] {
            let claims = AuthClaims {
                sub: "user@example.com".to_string(),
                exp: u64::MAX,
                aud: crate::auth::AUD_SESSION.to_string(),
                ver: 0,
                auth_type,
            };
            let resp = get_usage(
                State(state.clone()),
                claims,
                Query(UsageQuery { period_start: None }),
            )
            .await
            .unwrap_or_else(|_| panic!("usage handler returned an error"))
            .into_response();
            let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
                .await
                .expect("body");
            let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");

            assert_eq!(
                v["scope"], "api_key",
                "scope must describe the count, which is API key requests however it is read"
            );
            assert_eq!(
                v["counts_session_requests"], false,
                "the response must say the exclusion is deliberate"
            );
            assert_eq!(
                v["caller"],
                if auth_type == AuthType::ApiKey { "api_key" } else { "jwt" },
                "caller must describe the reader"
            );
        }
    }

    /// What an account with an unproven address is told, and that it is told
    /// something it can act on. A 403 whose remedy the reader has to guess is
    /// how an account becomes a support ticket, which is the whole risk of
    /// enforcing this at all.
    #[tokio::test]
    async fn the_gate_names_the_address_and_the_way_out() {
        let state = test_state();
        let email = "unverified@example.com";
        {
            let db = state.db.lock().await;
            db.create_user(email, "hash").expect("create");
        }

        let resp = verified_gate(&state, email)
            .await
            .expect("an unverified account must be refused");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);

        let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
            .await
            .expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["error"], "email_verification_required");
        assert_eq!(v["email"], email, "it says which address is unproven");
        assert_eq!(
            v["resend"], "/auth/resend-verification",
            "and where the way out is"
        );
        let detail = v["detail"].as_str().unwrap_or_default();
        assert!(
            detail.contains("Confirm your email") && detail.contains("Settings"),
            "the message has to tell a person what to do: {detail}"
        );
    }

    /// Verified accounts are not affected, which is the other half: a gate that
    /// refuses everyone is not a gate.
    #[tokio::test]
    async fn a_verified_account_passes_the_gate() {
        let state = test_state();
        let email = "verified@example.com";
        {
            let db = state.db.lock().await;
            db.create_user(email, "hash").expect("create");
            db.set_email_verified(email).expect("verify");
        }
        assert!(
            verified_gate(&state, email).await.is_none(),
            "a verified account must pass"
        );
    }

    /// An account with no row at all is refused rather than let through. A
    /// valid token for an account that does not exist should not happen, and
    /// the safe reading of a state that should not happen is to refuse.
    #[tokio::test]
    async fn an_account_that_does_not_exist_is_refused() {
        let state = test_state();
        assert!(
            verified_gate(&state, "ghost@example.com").await.is_some(),
            "no row means no proof"
        );
    }

    /// The gate goes on writing and spending, and nowhere else. Enumerated from
    /// the router source rather than from a list kept by hand, because a list of
    /// gated routes maintained beside the routes is a list that stops matching
    /// them. Reading and deleting must stay open: withholding data an account
    /// can already see helps nobody, and an account must always be able to take
    /// its own credentials away.
    #[test]
    fn the_gate_is_on_the_write_paths_and_not_the_read_ones() {
        let sources = [
            include_str!("routes.rs"),
            include_str!("api_keys.rs"),
            include_str!("webhooks.rs"),
            include_str!("email_alerts.rs"),
        ];
        // Matched on the call shape, not on the name. Matching the name alone
        // counted this test's own source, since routes.rs includes itself.
        let gated: Vec<&str> = sources
            .iter()
            .flat_map(|src| src.lines())
            .map(str::trim)
            .filter(|l| l.starts_with("if let Some(r) =") && l.contains("verified_gate"))
            .collect();
        assert_eq!(
            gated.len(),
            5,
            "expected the five write paths to be gated, found {}",
            gated.len()
        );

        // And the handlers that carry it, named so a removal is a deliberate
        // edit to this list rather than a line quietly disappearing.
        for (src, handler) in [
            (include_str!("api_keys.rs"), "pub async fn create_api_key"),
            (include_str!("webhooks.rs"), "pub async fn create_webhook"),
            (include_str!("email_alerts.rs"), "pub async fn upsert_email_alert"),
            (include_str!("routes.rs"), "async fn create_custom_rule"),
            (include_str!("routes.rs"), "async fn update_user_plan"),
        ] {
            let start = src.find(handler).unwrap_or_else(|| panic!("{handler} is gone"));
            let window = &src[start..(start + 700).min(src.len())];
            assert!(
                window.contains("verified_gate"),
                "{handler} must refuse an unverified account"
            );
        }
    }

    /// Self serve tier changes are refused unless the environment opts in, and
    /// the flag is the whole gate: with no payment processor connected, nothing
    /// else in the system can tell a paid tier from an unpaid one, so any
    /// account could grant itself enterprise with one request.
    #[test]
    fn self_serve_plan_change_is_off_unless_the_environment_opts_in() {
        // SAFETY: single threaded test, and the variable is read only here.
        unsafe { std::env::remove_var("ALLOW_SELF_SERVE_PLAN_CHANGE") };
        assert!(!self_serve_plan_change_enabled(), "unset must be closed");

        for off in ["", "0", "false", "no", "yes", "enabled"] {
            unsafe { std::env::set_var("ALLOW_SELF_SERVE_PLAN_CHANGE", off) };
            assert!(
                !self_serve_plan_change_enabled(),
                "{off:?} must not open the endpoint"
            );
        }
        for on in ["1", "true", "TRUE", "True"] {
            unsafe { std::env::set_var("ALLOW_SELF_SERVE_PLAN_CHANGE", on) };
            assert!(self_serve_plan_change_enabled(), "{on:?} must open it");
        }
        unsafe { std::env::remove_var("ALLOW_SELF_SERVE_PLAN_CHANGE") };
    }

    /// Moving down a tier gives nothing away and stays self serve, so the
    /// Billing page's downgrade to Free keeps working with the flag unset.
    /// Moving up needs the flag, because nothing can tell a paid tier from an
    /// unpaid one.
    #[test]
    fn only_a_raise_needs_the_flag() {
        use crate::plan::rank;
        // Same shape as the handler: is_raise = rank(requested) > rank(current)
        let is_raise = |current: &str, requested: &str| rank(requested) > rank(current);

        // Downgrades and sideways moves, allowed whatever the flag says.
        for (current, requested) in [
            ("enterprise", "free"),
            ("enterprise", "business"),
            ("business", "pro"),
            ("pro", "developer"),
            ("developer", "free"),
            ("free", "free"),
            ("pro", "pro"),
        ] {
            assert!(
                !is_raise(current, requested),
                "{current} to {requested} must not need the flag"
            );
        }

        // Raises, refused unless the flag is on.
        for (current, requested) in [
            ("free", "enterprise"),
            ("free", "developer"),
            ("developer", "pro"),
            ("pro", "business"),
            ("business", "enterprise"),
        ] {
            assert!(
                is_raise(current, requested),
                "{current} to {requested} must need the flag"
            );
        }
    }

    /// The tier names the backend accepts must match the tiers the frontend
    /// offers, exactly. `starter` used to exist on the backend only, which is
    /// why the frontend had to normalise it away.
    #[test]
    fn the_backend_tier_set_matches_the_frontend() {
        let frontend = ["free", "developer", "pro", "business", "enterprise"];
        for tier in frontend {
            assert!(
                VALID_PLANS.contains(&tier),
                "frontend offers {tier}, backend does not accept it"
            );
        }
        for tier in VALID_PLANS {
            assert!(
                frontend.contains(tier),
                "backend accepts {tier}, which the frontend does not offer"
            );
        }
        assert!(!VALID_PLANS.contains(&"starter"), "starter is gone");
    }

    /// A component with no recorded history must not read as downtime. It used
    /// to report 0 percent, which on a page of components at 100 percent looks
    /// The status page exists to say whether the product works. An astronomy
    /// picture failing to fetch is not that, and it used to put "Partial
    /// Outage" at the top of a page a satellite operator reads as a statement
    /// about space weather data.
    ///
    /// Both directions are asserted together on purpose. Excluding the wrong
    /// set would be silent otherwise: a green page during a real NOAA outage is
    /// the worse failure of the two, and it is the one nobody would report.
    #[tokio::test]
    async fn auxiliary_feeds_cannot_move_the_overall_status() {
        for (component, is_aux) in [
            ("nasa_apod", true),
            ("nasa_epic", true),
            ("nasa_neo", true),
            ("nasa_exoplanets", true),
            ("noaa_kp", false),
            ("noaa_imf", false),
            ("noaa_alerts", false),
            ("iss", false),
            ("celestrak", false),
        ] {
            assert_eq!(
                crate::db::is_auxiliary(component),
                is_aux,
                "{component} is on the wrong side of the auxiliary line"
            );
        }

        // The exclusion itself, which the two assertions above do not reach: a
        // degraded auxiliary feed must leave the status alone and a degraded
        // product feed must not.
        let quiet_apod = [
            ("noaa_kp", "operational", Some(0)),
            ("nasa_apod", "degraded", Some(0)),
        ];
        let quiet_kp = [
            ("noaa_kp", "degraded", Some(0)),
            ("nasa_apod", "operational", Some(0)),
        ];
        assert!(
            crate::db::all_product_components_operational(&quiet_apod),
            "an astronomy picture must not decide whether the product works"
        );
        assert!(
            !crate::db::all_product_components_operational(&quiet_kp),
            "a green page during a NOAA outage is the worse failure of the two"
        );

        // Everything is still published, whichever side of the line it is on.
        let state = test_state();
        let resp = health(State(state.clone())).await;
        let bytes = axum::body::to_bytes(resp.into_response().into_body(), 1 << 20)
            .await
            .expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let comps = v["components"].as_object().expect("components");
        for component in crate::db::AUXILIARY {
            assert!(
                comps.contains_key(component),
                "{component} stopped deciding the status and must still be published"
            );
        }
    }

    /// A component whose status is published and whose history is not is half
    /// published. `noaa_alerts` was in `/api/health` and absent from
    /// `/api/health/uptime` from the day it was added until 2026-09-01, so the
    /// status page showed it with an empty strip.
    #[tokio::test]
    async fn every_published_component_has_an_uptime_entry() {
        let state = test_state();
        let h = health(State(state.clone())).await;
        let hb = axum::body::to_bytes(h.into_response().into_body(), 1 << 20)
            .await
            .expect("body");
        let hv: serde_json::Value = serde_json::from_slice(&hb).expect("json");

        let Ok(u) = uptime(State(state.clone())).await else {
            panic!("uptime handler failed");
        };
        let ub = axum::body::to_bytes(u.into_response().into_body(), 1 << 20)
            .await
            .expect("body");
        let uv: serde_json::Value = serde_json::from_slice(&ub).expect("json");

        let published = hv["components"].as_object().expect("components");
        let with_history = uv["components"].as_object().expect("components");
        let missing: Vec<&String> = published
            .keys()
            .filter(|k| !with_history.contains_key(*k))
            .collect();
        assert!(
            missing.is_empty(),
            "published with no uptime history: {missing:?}"
        );
    }

    /// like three months of outage rather than an absence of records.
    #[tokio::test]
    async fn a_component_with_no_history_reports_null_not_zero() {
        let state = test_state();
        let now_ts = chrono::Utc::now().timestamp();
        {
            let db = state.db.lock().await;
            // Two days of history for one component, all healthy.
            for day in 0..2i64 {
                for sample in 0..5i64 {
                    db.insert_health_snapshot(
                        "backend_api",
                        now_ts - day * 86_400 - sample * 300,
                        Some("operational"),
                    )
                    .expect("snapshot");
                }
            }
        }

        let Ok(resp) = uptime(State(state.clone())).await else {
            panic!("uptime handler failed");
        };
        let bytes = axum::body::to_bytes(resp.into_response().into_body(), 1 << 20)
            .await
            .expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let comps = &v["components"];

        // Recorded: a real figure, and the window it covers.
        assert!(
            comps["backend_api"]["uptime_pct"].is_f64(),
            "a recorded component must report a percentage"
        );
        assert_eq!(comps["backend_api"]["recorded_days"], 2);

        // Never recorded: null, and no days claimed.
        assert!(
            comps["noaa_imf"]["uptime_pct"].is_null(),
            "no history must be null, not zero: {}",
            comps["noaa_imf"]["uptime_pct"]
        );
        assert_eq!(comps["noaa_imf"]["recorded_days"], 0);

        // And its strip is entirely no_data rather than outage.
        let days = comps["noaa_imf"]["days"].as_array().expect("days");
        assert_eq!(days.len(), 90);
        assert!(
            days.iter().all(|d| d["status"] == "no_data"),
            "unrecorded days must be no_data, never outage"
        );
    }

    /// A gap in the record is downtime, not an absence of record.
    ///
    /// This is the whole of AUD-021. Dividing operational samples by samples
    /// present meant an outage removed rows from both halves of the fraction
    /// and cancelled itself out, so `uptime_pct` was structurally incapable of
    /// falling below 100 however long the backend was down. Half a day of
    /// samples followed by half a day of silence has to read as about half.
    #[tokio::test]
    async fn silence_counts_against_a_component_that_had_started_recording() {
        let state = test_state();
        let now_ts = chrono::Utc::now().timestamp();
        {
            let db = state.db.lock().await;
            // Twelve hours of samples ending twelve hours ago, then nothing.
            let mut ts = now_ts - 86_400;
            while ts <= now_ts - 43_200 {
                db.insert_health_snapshot("noaa_kp", ts, Some("operational"))
                    .expect("snapshot");
                ts += 300;
            }
        }

        let v = call_uptime(&state).await;
        let pct = v["components"]["noaa_kp"]["uptime_pct"]
            .as_f64()
            .expect("a percentage");
        assert!(
            (40.0..60.0).contains(&pct),
            "half a day recorded and half a day silent must read as about half, got {pct}"
        );
    }

    /// Nothing was expected before a component's first sample, so a component
    /// added yesterday still reads as ninety days of no data. `2623cf6` decided
    /// that on purpose after six NOAA components showed three months of
    /// downtime on the day they were added, and an expected count denominator
    /// only keeps that decision because expectation starts at the first sample.
    #[tokio::test]
    async fn history_before_the_first_sample_is_not_held_against_a_component() {
        let state = test_state();
        let now_ts = chrono::Utc::now().timestamp();
        {
            let db = state.db.lock().await;
            for k in 0..8i64 {
                db.insert_health_snapshot("noaa_dst", now_ts - k * 300, Some("operational"))
                    .expect("snapshot");
            }
        }

        let v = call_uptime(&state).await;
        let comp = &v["components"]["noaa_dst"];
        assert_eq!(comp["uptime_pct"], 100.0, "a clean short history is 100, not 2");
        assert_eq!(comp["recorded_days"], 1, "and it claims one day, not ninety");
        let days = comp["days"].as_array().expect("days");
        assert!(
            days[..88].iter().all(|d| d["status"] == "no_data"),
            "the days before it existed stay no_data"
        );
    }

    /// The strip is labelled in days, so the buckets have to be days. They were
    /// `(now - ts) / 86400`, a rolling offset from request time, which put the
    /// same historical sample in a different cell depending on the hour the
    /// page was loaded. A full UTC day of samples has to fill exactly one cell.
    #[tokio::test]
    async fn a_day_of_samples_fills_exactly_one_calendar_cell() {
        let state = test_state();
        let now_ts = chrono::Utc::now().timestamp();
        let yesterday = now_ts / 86_400 - 1;
        {
            let db = state.db.lock().await;
            for k in 0..288i64 {
                db.insert_health_snapshot("noaa_xray", yesterday * 86_400 + k * 300, Some("operational"))
                    .expect("snapshot");
            }
            // Half the same day, so the cell has to read half. The cell's own
            // denominator is what this pins: dividing by samples present would
            // call a half covered day complete, which is the outage hiding in
            // one cell rather than in the total.
            for k in 0..144i64 {
                db.insert_health_snapshot("noaa_imf", yesterday * 86_400 + k * 300, Some("operational"))
                    .expect("snapshot");
            }
        }

        let v = call_uptime(&state).await;
        let days = v["components"]["noaa_xray"]["days"].as_array().expect("days");
        // Index 89 is today, so 88 is yesterday.
        assert_eq!(
            days[88]["uptime_pct"], 100.0,
            "a full UTC day of samples belongs to that day's cell, whole"
        );

        let half = v["components"]["noaa_imf"]["days"].as_array().expect("days");
        assert_eq!(
            half[88]["uptime_pct"], 50.0,
            "half a day of samples is a half full cell, not a full one"
        );
    }

    /// A liveness component records no verdict, so the row being there is the
    /// entire observation and the percentage counts rows rather than statuses.
    /// `backend_api` used to write the literal "operational" forever.
    #[tokio::test]
    async fn a_liveness_component_is_measured_by_presence() {
        let state = test_state();
        let now_ts = chrono::Utc::now().timestamp();
        {
            let db = state.db.lock().await;
            for k in 0..8i64 {
                db.insert_health_snapshot("backend_api", now_ts - k * 300, None)
                    .expect("snapshot");
            }
        }

        let v = call_uptime(&state).await;
        let comp = &v["components"]["backend_api"];
        assert_eq!(
            comp["measures"], "liveness",
            "the page has to be able to say what this number covers"
        );
        assert_eq!(comp["uptime_pct"], 100.0, "rows with no status still count");
        for name in crate::db::LIVENESS_ONLY {
            assert_eq!(
                v["components"][name]["measures"], "liveness",
                "{name} is liveness only and must say so"
            );
        }
    }

    /// The four unauthenticated tools stay unauthenticated.
    #[tokio::test]
    async fn mcp_public_tools_need_no_token() {
        let state = test_state();
        for tool in ["get_current_kp", "get_solar_wind", "get_health"] {
            let v = call_tool(&state, tool, None).await;
            assert!(!is_auth_error(&v), "{tool} must not require a token, got {v}");
        }
    }
}
