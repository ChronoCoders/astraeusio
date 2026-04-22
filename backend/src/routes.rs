use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, MutexGuard};

use anyhow::anyhow;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use tracing::info;

use crate::{auth, db::Db};

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
    cache.lock().await.insert(key, (Instant::now(), val.clone()));
    Ok(Json(val))
}

// ── State ─────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AppState {
    pub client: reqwest::Client,
    pub db: Arc<Mutex<Db>>,
    pub ml_url: String,
    pub cache: Arc<Mutex<CacheMap>>,
    pub jwt_secret: String,
}

impl AppState {
    pub fn new(client: reqwest::Client, db: Db, ml_url: String, jwt_secret: String) -> Self {
        Self {
            client,
            db: Arc::new(Mutex::new(db)),
            ml_url,
            cache: Arc::new(Mutex::new(HashMap::new())),
            jwt_secret,
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

async fn lock_db(db: &Arc<Mutex<Db>>) -> MutexGuard<'_, Db> {
    db.lock().await
}

// ── Router ────────────────────────────────────────────────────────────────────

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/auth/register", post(auth::register))
        .route("/auth/login", post(auth::login))
        .route("/api/apod", get(get_apod))
        .route("/api/neo", get(get_neo))
        .route("/api/epic", get(get_epic))
        .route("/api/exoplanets", get(get_exoplanets))
        .route("/api/kp", get(get_kp))
        .route("/api/solar-wind", get(get_solar_wind))
        .route("/api/xray", get(get_xray))
        .route("/api/alerts", get(get_alerts))
        .route("/api/iss", get(get_iss))
        .route("/api/kp-forecast", get(get_kp_forecast))
        .route("/api/anomalies", get(get_anomalies))
        .with_state(state)
}

// ── NASA handlers ─────────────────────────────────────────────────────────────

async fn get_apod(State(s): State<AppState>) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "apod", Duration::from_secs(60), || async {
        let val = lock_db(&s.db).await.get_apod_latest()?;
        info!("api/apod: served from db");
        Ok(val)
    })
    .await
}

async fn get_neo(State(s): State<AppState>) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "neo", Duration::from_secs(60), || async {
        let val = lock_db(&s.db).await.get_neo_recent()?;
        info!("api/neo: served from db");
        Ok(val)
    })
    .await
}

async fn get_epic(State(s): State<AppState>) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "epic", Duration::from_secs(60), || async {
        let val = lock_db(&s.db).await.get_epic_latest()?;
        info!("api/epic: served from db");
        Ok(val)
    })
    .await
}

async fn get_exoplanets(State(s): State<AppState>) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "exoplanets", Duration::from_secs(3600), || async {
        let val = lock_db(&s.db).await.get_exoplanets_all()?;
        info!("api/exoplanets: served from db");
        Ok(val)
    })
    .await
}

// ── NOAA handlers ─────────────────────────────────────────────────────────────

async fn get_kp(State(s): State<AppState>) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "kp", Duration::from_secs(10), || async {
        let val = lock_db(&s.db).await.get_kp_recent()?;
        info!("api/kp: served from db");
        Ok(val)
    })
    .await
}

async fn get_solar_wind(State(s): State<AppState>) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "solar-wind", Duration::from_secs(10), || async {
        let val = lock_db(&s.db).await.get_solar_wind_recent()?;
        info!("api/solar-wind: served from db");
        Ok(val)
    })
    .await
}

async fn get_xray(State(s): State<AppState>) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "xray", Duration::from_secs(30), || async {
        let val = lock_db(&s.db).await.get_xray_recent()?;
        info!("api/xray: served from db");
        Ok(val)
    })
    .await
}

async fn get_alerts(State(s): State<AppState>) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "alerts", Duration::from_secs(60), || async {
        let val = lock_db(&s.db).await.get_alerts_recent()?;
        info!("api/alerts: served from db");
        Ok(val)
    })
    .await
}

// ── ISS handler ───────────────────────────────────────────────────────────────

async fn get_iss(State(s): State<AppState>) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "iss", Duration::from_secs(3), || async {
        let val = lock_db(&s.db).await.get_iss_latest()?;
        info!("api/iss: served from db");
        Ok(val)
    })
    .await
}

// ── ML forecast handler ───────────────────────────────────────────────────────

async fn get_kp_forecast(State(s): State<AppState>) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "kp-forecast", Duration::from_secs(180), || async {
        let readings = lock_db(&s.db).await.get_recent_kp(7)?;

        if readings.is_empty() {
            return Err(anyhow!("no Kp data in database — poller initializing").into());
        }

        info!("kp-forecast: sending {} readings to ML service", readings.len());

        let body = serde_json::json!({ "readings": readings });
        let resp = s
            .client
            .post(format!("{}/predict", s.ml_url))
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        let payload: serde_json::Value = resp.json().await?;

        if !status.is_success() {
            return Err(anyhow!("ML service error {status}: {payload}").into());
        }

        // Persist forecast for anomaly detection (3-hour horizon from now).
        if let Some(kp) = payload.get("predicted_kp").and_then(|v| v.as_f64()) {
            let forecast_ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64
                + 3 * 3600;
            let kp_e2 = (kp * 100.0).round() as i64;
            if let Err(e) = lock_db(&s.db).await.insert_kp_forecast(forecast_ts, kp_e2) {
                tracing::warn!("kp-forecast: failed to persist to db: {e}");
            }
        }

        Ok(payload)
    })
    .await
}

// ── Anomaly handler ───────────────────────────────────────────────────────────

async fn get_anomalies(State(s): State<AppState>) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "anomalies", Duration::from_secs(30), || async {
        let val = lock_db(&s.db).await.get_anomalies_recent()?;
        info!("api/anomalies: served from db");
        Ok(val)
    })
    .await
}
