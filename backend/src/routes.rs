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
    pub mailer: Option<Arc<dyn mailer::Sender>>,
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
        mailer: Option<Arc<dyn mailer::Sender>>,
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

    // Every key here comes from a declaration in db.rs. It used to type
    // "backend_api", "ml_forecast", "database" and "celestrak" at this site,
    // which made this the third hand-kept list beside the uptime handler's and
    // the writers', and this is the one that decides what the public page shows
    // at all: a component missing here is invisible whatever history exists.
    let mut components = serde_json::Map::new();
    components.insert(
        crate::db::BACKEND_COMPONENT.into(),
        serde_json::json!({ "status": "operational", "last_checked": now }),
    );
    components.insert(
        crate::db::ML_COMPONENT.into(),
        serde_json::json!({ "status": ml_status, "last_checked": now }),
    );
    components.insert(
        crate::db::DATABASE_COMPONENT.into(),
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
        crate::db::CELESTRAK_COMPONENT.into(),
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
        //
        // One list, composed from the declarations the writers use, rather than
        // the hand-kept `["backend_api", "ml_forecast", "database", "celestrak"]`
        // that used to sit here beside two chained constants. That arrangement
        // could disagree with the writer and nothing would say so.
        //
        // "nasa" is deliberately absent: it was split into one component per
        // feed. Its historical rows stay in health_snapshots and stop being
        // rendered, which is what an identifier leaving `health_components`
        // means.
        let components = crate::db::health_components().into_iter();

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

/// The same rows the report endpoints return, as CSV.
///
/// Deliberately ungated. It required `developer` while `/api/reports/kp` and
/// `/api/reports/solar-wind` returned the same rows over the same window to
/// anyone signed in, so the gate cost a caller one extra request and bought
/// nothing. It was selling a format.
///
/// Dropping it rather than gating the other two, because the pricing page
/// already promises Kp and solar wind data on the free tier and lists CSV
/// export on no tier at all. Gating all four would take something away that the
/// site currently offers, which is a pricing decision and not a fix.
///
/// The real free-versus-paid line the pricing page claims is `delay60`, a sixty
/// second delay on free-tier data, and nothing in this service implements it.
/// That is recorded in the backlog rather than invented here.
async fn get_report_export(
    State(s): State<AppState>,
    _claims: AuthClaims,
    Query(q): Query<ReportQuery>,
) -> Result<Response, AppError> {
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

    /// The four report routes are gated the same way, which is to say not at all.
    ///
    /// The export required `developer` while `/api/reports/kp` and
    /// `/api/reports/solar-wind` returned the same rows over the same window to
    /// anyone signed in, so the gate cost a caller one extra request and bought
    /// nothing. It was selling a format, and the pricing page sells Kp and solar
    /// wind on the free tier and lists CSV export on no tier at all.
    ///
    /// Asserted as one rule over all four rather than as the absence of one
    /// line, because the failure this had was three routes agreeing and a fourth
    /// not, which nothing noticed for as long as it existed.
    #[test]
    fn the_report_routes_are_gated_alike() {
        let src = include_str!("routes.rs");
        for handler in [
            "async fn get_report_summary",
            "async fn get_report_export",
            "async fn get_report_kp",
            "async fn get_report_solar_wind",
        ] {
            let start = src.find(handler).unwrap_or_else(|| panic!("{handler} is gone"));
            // The handler's own body, to the start of the next item.
            let rest = &src[start..];
            let end = rest[1..].find("\nasync fn ").map_or(rest.len(), |i| i + 1);
            let body = &rest[..end];
            assert!(
                !body.contains("plan_gate("),
                "{handler} is plan gated while the others are not; the four read the \
                 same rows over the same window, so a gate on one of them is a gate on \
                 the format rather than on the data"
            );
        }
    }

    /// The figure that looks forward is named for looking forward.
    ///
    /// `asteroid_approaches` counted today through today plus the range, inside
    /// a payload whose other four figures describe the range just past, under a
    /// card the page labels as a summary of the selected period.
    #[test]
    fn the_forward_looking_count_is_named_for_its_own_window() {
        let src = include_str!("db.rs");
        assert!(
            src.contains("\"upcoming_approaches\": asteroid_count"),
            "the field has to say which way it looks"
        );
        assert!(
            !src.contains("\"asteroid_approaches\":"),
            "the old name must be gone, not shadowed"
        );

        // And the page reads the name the backend sends.
        let page = include_str!("../../frontend/src/components/ReportsPage.jsx");
        assert!(
            page.contains("summary.upcoming_approaches"),
            "the page must read the field the API returns"
        );
        assert!(
            !page.contains("summary.asteroid_approaches"),
            "a rename the page did not follow renders an empty card"
        );
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
            // Two days of history for one component, all healthy. Today's
            // samples are pinned inside today and yesterday's around midday, so
            // the fixture covers exactly two days whatever the hour.
            let midday_yesterday = now_ts - now_ts.rem_euclid(86_400) - 86_400 + 43_200;
            let mut times = samples_within_today(now_ts, 5);
            times.extend((0..5i64).map(|sample| midday_yesterday - sample * 300));
            for ts in times {
                db.insert_health_snapshot("backend_api", ts, Some("operational"))
                    .expect("snapshot");
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

    /// A run of `count` sample times ending at now, all inside today's UTC day.
    ///
    /// These fixtures used to be written as `now - k * 300`, which slides into
    /// the previous UTC day whenever the suite runs less than `k * 300` seconds
    /// after midnight. `recorded_days` then counts a day the test did not intend
    /// and the expected 1 or 2 is wrong: correct for all but the first forty
    /// minutes of every day, which is why it stood until it failed at 00:13 UTC
    /// on 2026-09-04. The clock found it, not a change to anything it covers.
    ///
    /// The spacing compresses to fit rather than the run reaching backwards, so
    /// the number of days covered is a property of the fixture and not of the
    /// hour the suite happens to run. Deriving the expected count from the
    /// timestamps instead would only restate the handler's own arithmetic,
    /// including the part where a day holding less than one poll interval after
    /// the first sample contributes nothing.
    ///
    /// Within the first seven seconds of a UTC day there is no room even at one
    /// second spacing, and the run starts up to seven seconds ahead of now. That
    /// is bounded and harmless: `samples_due` measures to `now` either way, and
    /// the samples still fall in today's bucket.
    fn samples_within_today(now: i64, count: i64) -> Vec<i64> {
        let day_start = now - now.rem_euclid(86_400);
        let span = count - 1;
        let step = ((now - day_start) / span.max(1)).clamp(1, 300);
        let base = now.max(day_start + span * step);
        (0..count).map(|k| base - k * step).collect()
    }

    /// The property the two fixtures below rest on, checked at the hours that
    /// broke them rather than at whatever hour the suite happens to run.
    ///
    /// Without this, a fix for a bug that only appears in the first forty
    /// minutes of a UTC day is only verified if the suite is run inside those
    /// forty minutes, which is exactly when nobody is running it.
    #[test]
    fn a_fixture_run_stays_inside_one_utc_day() {
        const MIDNIGHT: i64 = 1_788_480_000; // 20700 * 86400, a UTC midnight
        for offset in [0i64, 3, 7, 60, 300, 780, 1_200, 2_400, 43_200, 86_399] {
            for count in [5i64, 8] {
                let now = MIDNIGHT + offset;
                let times = samples_within_today(now, count);
                assert_eq!(times.len() as i64, count, "at +{offset}s");
                // Distinct instants, not one instant repeated. Without this the
                // spacing floor could go to zero and the run would collapse
                // onto a single timestamp while every other assertion here and
                // in both fixtures carried on holding.
                assert_eq!(
                    times.iter().collect::<std::collections::HashSet<_>>().len() as i64,
                    count,
                    "{count} samples at +{offset}s are not {count} distinct instants: {times:?}"
                );
                for ts in &times {
                    assert_eq!(
                        ts.div_euclid(86_400),
                        now.div_euclid(86_400),
                        "{count} samples at +{offset}s produced {ts}, outside the day of {now}"
                    );
                }
                // Newest first, and never further ahead of now than the seven
                // second floor allows.
                assert!(
                    times[0] - now < count,
                    "{count} samples at +{offset}s start {} ahead of now",
                    times[0] - now
                );
            }
        }
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
            for ts in samples_within_today(now_ts, 8) {
                db.insert_health_snapshot("noaa_dst", ts, Some("operational"))
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

    /// No health snapshot may be written for a name the readers do not know.
    ///
    /// This is the guard the old one only looked like. `every_published_
    /// component_has_an_uptime_entry` compared `/api/health`'s output against
    /// `/api/health/uptime`'s output, two runtime outputs, so a component added
    /// to the writer and missed by both readers passed. The authoritative list
    /// is the one in the source, so this reads the writer's source instead.
    ///
    /// Every `WriteCmd::HealthSnapshot` site must take its component from a
    /// declaration, never from a literal typed at the site. A literal is
    /// exactly the drift being refused: it names something `health_components`
    /// cannot know about, so the strip for it would be empty forever.
    #[test]
    fn no_health_snapshot_is_written_for_an_undeclared_component() {
        for (file, src) in [
            ("poller.rs", include_str!("poller.rs")),
            ("routes.rs", include_str!("routes.rs")),
            ("db_writer.rs", include_str!("db_writer.rs")),
        ] {
            let mut sites = 0;
            for (i, _) in src.match_indices("WriteCmd::HealthSnapshot") {
                // The `component:` field of this construction, which is the
                // next occurrence of that field name after the site.
                let rest = &src[i..];
                let Some(f) = rest.find("component:") else { continue };
                let line_end = rest[f..].find('\n').map_or(rest.len(), |e| f + e);
                let field = &rest[f..line_end];
                sites += 1;
                assert!(
                    !field.contains('"'),
                    "{file}: a health snapshot is written for a literal name: {field}. \
                     Take it from a declaration in db.rs so health_components() covers it."
                );
            }
            // A guard that finds no sites guards nothing, so the count is
            // asserted for the file that has them. Two: the health cycle's
            // single loop over `health_samples`, and the alerts poller writing
            // its own liveness verdict. It was three until those three loops
            // became one, which is why this is a floor with a reason rather
            // than a number that drifts with refactoring.
            if file == "poller.rs" {
                assert!(
                    sites >= 2,
                    "expected the health cycle and the alerts poller to write snapshots, found {sites}"
                );
            }
        }
    }

    /// Everything the writers can produce has somewhere to be read.
    ///
    /// Enumerated from `health_components`, which is composed from the same
    /// declarations the writers use, rather than from a payload.
    #[tokio::test]
    async fn every_declared_component_has_an_uptime_entry() {
        let state = test_state();
        let v = call_uptime(&state).await;
        let with_history = v["components"].as_object().expect("components");

        let declared = crate::db::health_components();
        assert!(
            !declared.is_empty(),
            "an empty declaration would make this pass vacuously"
        );
        let missing: Vec<&&str> = declared
            .iter()
            .filter(|c| !with_history.contains_key(**c))
            .collect();
        assert!(missing.is_empty(), "declared with no uptime entry: {missing:?}");

        // And the four the old hand-kept list held are still among them, so
        // composing the list did not quietly drop one.
        for name in ["backend_api", "database", "ml_forecast", "celestrak"] {
            assert!(
                declared.contains(&name),
                "{name} left the declared set when the lists were composed"
            );
        }
    }

    /// The manifest, as the handler's own clients read it.
    ///
    /// `(name, description)` for every tool `tools/list` advertises. Everything
    /// below enumerates from this rather than from a list typed into a test:
    /// `mcp_public_tools_need_no_token` used to hold three names by hand under a
    /// doc comment that said four, and neither the comment nor the list could
    /// have noticed the other was wrong.
    fn advertised_tools() -> Vec<(String, String)> {
        let v: serde_json::Value = serde_json::from_str(MCP_TOOLS).expect("MCP_TOOLS is json");
        v["tools"]
            .as_array()
            .expect("tools array")
            .iter()
            .map(|t| {
                (
                    t["name"].as_str().expect("tool name").to_string(),
                    t["description"].as_str().unwrap_or_default().to_string(),
                )
            })
            .collect()
    }

    /// Every tool the manifest advertises is one the handler answers.
    ///
    /// The protected thing here is a caller's ability to call what `tools/list`
    /// told them exists. This direction catches the manifest advertising
    /// something the dispatch has no arm for, which reaches the caller as
    /// "unknown tool" on a name the server itself published.
    ///
    /// It is deliberately only half of the contract. Enumerating from the
    /// manifest cannot see an arm the manifest omits, in the same way that
    /// enumerating from a validator's call sites could not see the path that
    /// called no validator. `no_mcp_tool_answers_unadvertised` is the other
    /// direction.
    ///
    /// A token is supplied, so the three authenticated tools reach their
    /// handlers rather than stopping at the auth check. An internal error is
    /// accepted: `get_kp_forecast` reaches for the ML service, which is not
    /// running here, and -32603 still means the arm exists. Only -32601 is a
    /// failure, because only -32601 means nothing answered to that name.
    #[tokio::test]
    async fn every_advertised_mcp_tool_answers() {
        let state = test_state();
        let token = session_jwt("mcp@example.com", SECRET, 0).expect("mint");
        {
            let db = state.db.lock().await;
            db.create_user("mcp@example.com", "hash").expect("user");
        }
        let advertised = advertised_tools();
        assert_eq!(
            advertised.len(),
            7,
            "the manifest advertises {} tools; if that is intended, say so here",
            advertised.len()
        );
        for (name, _) in &advertised {
            let v = call_tool(&state, name, Some(&token)).await;
            let code = v.get("error").and_then(|e| e.get("code")).and_then(|c| c.as_i64());
            assert_ne!(
                code,
                Some(-32601),
                "{name} is advertised by tools/list and the dispatch has no arm for it: {v}"
            );
        }
    }

    /// Nothing answers to a name the manifest does not carry.
    ///
    /// The other half of the contract, and the weaker of the two guards. It
    /// reads the text of the dispatch rather than exercising it, so it can see a
    /// name and not whether that arm is reachable. It is here as a net for an
    /// arm added without a manifest entry, which would be a surface no client is
    /// told about and, for the authenticated three, an undocumented one.
    ///
    /// Whitespace is removed before matching. A raw text scan misses any site
    /// rustfmt has wrapped, which has now cost two guards in this repository:
    /// the confidence interval sweep read a sentence that wrapped as absent, and
    /// the credential scan found three of four call sites because one call was
    /// spread over three lines.
    #[test]
    fn no_mcp_tool_answers_unadvertised() {
        let src: String = include_str!("routes.rs")
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();

        // The dispatch, bounded by its own two ends: the arm that opens it and
        // the unknown-tool arm that closes it. Bounding on the function name
        // would drag in every other match in this file.
        // Past the arm, not at it: the region must hold the tool names and not
        // the JSON-RPC method that introduces them.
        const OPEN: &str = r#""tools/call"=>"#;
        let open = src.find(OPEN).expect("the tools/call arm") + OPEN.len();
        let close = src[open..]
            .find("_=>McpResp::err(id,-32601")
            .expect("the unknown tool arm that ends the dispatch")
            + open;
        let dispatch = &src[open..close];

        // A quoted string followed by `=>` or `|` is a match arm. Inside this
        // region the only other string literals sit in json! bodies and call
        // arguments, where neither can follow.
        let mut arms: Vec<String> = Vec::new();
        let bytes: Vec<char> = dispatch.chars().collect();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == '"' {
                let start = i + 1;
                let mut j = start;
                while j < bytes.len() && bytes[j] != '"' {
                    j += 1;
                }
                if j + 1 < bytes.len() {
                    let after: String = bytes[j + 1..(j + 3).min(bytes.len())].iter().collect();
                    if after.starts_with("=>") || after.starts_with('|') {
                        arms.push(bytes[start..j].iter().collect());
                    }
                }
                i = j + 1;
            } else {
                i += 1;
            }
        }
        arms.sort();
        arms.dedup();

        // A scan that matched nothing would pass here without asserting
        // anything at all. Seven is what the manifest carries.
        assert!(
            arms.len() >= 7,
            "the dispatch scan found {} arms, which is too few to conclude anything from: {arms:?}",
            arms.len()
        );

        let advertised: Vec<String> = advertised_tools().into_iter().map(|(n, _)| n).collect();
        let unadvertised: Vec<&String> =
            arms.iter().filter(|a| !advertised.contains(a)).collect();
        assert!(
            unadvertised.is_empty(),
            "the dispatch answers to names tools/list does not advertise: {unadvertised:?}. \
             Add them to MCP_TOOLS or remove the arm."
        );
    }

    /// The published discovery card advertises what this endpoint actually
    /// serves.
    ///
    /// `frontend/public/.well-known/mcp/server-card.json` is a static file that
    /// names `https://astraeusio.com/mcp` as its transport, so it is a manifest
    /// for this handler written outside this crate. It sat at eleven tools while
    /// the endpoint served seven: four were fiction and had never had an arm,
    /// and two more were stale names for tools that do exist. An agent reading
    /// the card and calling any of the six got "unknown tool" from a name the
    /// site itself published.
    ///
    /// `every_advertised_mcp_tool_answers` is exactly the guard for that and did
    /// not catch it, because it enumerates from `MCP_TOOLS` and this is a second
    /// manifest for the same endpoint. Being outside the Rust tree is why it
    /// survived three passes over the tool lists.
    ///
    /// Parsed rather than scanned. The sibling scans here strip whitespace
    /// before matching because a wrapped line defeats a text search, and that has
    /// cost two guards in this repository. A JSON parser makes the question moot:
    /// whitespace is not part of the document, so there is no formatting of this
    /// file that can hide an entry from this test. Text scanning is the fallback
    /// for source that has no parser, and this file has one.
    #[test]
    fn the_server_card_advertises_what_the_endpoint_serves() {
        const CARD: &str =
            include_str!("../../frontend/public/.well-known/mcp/server-card.json");
        let card: serde_json::Value = serde_json::from_str(CARD).expect("the card is json");

        // The card is only this endpoint's manifest while it points here. If the
        // transport moves, the comparison below is comparing two unrelated
        // things and should be revisited rather than quietly kept passing.
        let transport = card["transport"][0]["url"].as_str().unwrap_or_default();
        assert!(
            transport.ends_with("/mcp"),
            "the card's transport is {transport}, which is not this handler; \
             this test is comparing the wrong two lists"
        );

        let mut published: Vec<(String, String)> = card["capabilities"]["tools"]
            .as_array()
            .expect("the card lists tools")
            .iter()
            .map(|t| {
                (
                    t["name"].as_str().unwrap_or_default().to_string(),
                    t["description"].as_str().unwrap_or_default().to_string(),
                )
            })
            .collect();

        // A card that parsed to nothing would agree with an empty manifest.
        assert!(
            published.len() >= 7,
            "the card lists {} tools, too few to conclude anything from",
            published.len()
        );

        let mut advertised = advertised_tools();
        published.sort();
        advertised.sort();

        let only_on_the_card: Vec<&(String, String)> =
            published.iter().filter(|t| !advertised.contains(t)).collect();
        assert!(
            only_on_the_card.is_empty(),
            "the card publishes tools this endpoint does not serve: {only_on_the_card:?}. \
             Every name here reaches a caller as a promise."
        );

        let only_in_the_manifest: Vec<&(String, String)> =
            advertised.iter().filter(|t| !published.contains(t)).collect();
        assert!(
            only_in_the_manifest.is_empty(),
            "the endpoint serves tools the card does not publish: {only_in_the_manifest:?}. \
             Discovery is how an agent finds these, so an omission hides them."
        );
    }

    /// The tools the manifest calls unauthenticated stay unauthenticated.
    ///
    /// Taken from the manifest's own wording rather than from a list here. The
    /// hand-written list this replaces held three names beneath a comment that
    /// said four, and `get_kp_forecast` was the one missing.
    #[tokio::test]
    async fn mcp_public_tools_need_no_token() {
        let state = test_state();
        let public: Vec<String> = advertised_tools()
            .into_iter()
            .filter(|(_, d)| d.contains("no auth required"))
            .map(|(n, _)| n)
            .collect();
        assert_eq!(
            public.len(),
            4,
            "the manifest marks {} tools as needing no auth: {public:?}",
            public.len()
        );
        for tool in &public {
            let v = call_tool(&state, tool, None).await;
            assert!(!is_auth_error(&v), "{tool} must not require a token, got {v}");
        }
    }

    /// And the ones it says need a token are refused without one.
    ///
    /// The manifest is a claim about the handler, so both halves of the claim
    /// are worth checking: `mcp_public_tools_need_no_token` would still pass if
    /// every tool were public.
    #[tokio::test]
    async fn mcp_tools_that_say_they_need_a_token_require_one() {
        let state = test_state();
        let guarded: Vec<String> = advertised_tools()
            .into_iter()
            .filter(|(_, d)| d.contains("Requires Bearer token"))
            .map(|(n, _)| n)
            .collect();
        assert_eq!(
            guarded.len(),
            3,
            "the manifest marks {} tools as needing a token: {guarded:?}",
            guarded.len()
        );
        for tool in &guarded {
            let v = call_tool(&state, tool, None).await;
            assert!(is_auth_error(&v), "{tool} answered without a token: {v}");
        }
    }
}
