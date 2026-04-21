use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, MutexGuard};

use anyhow::anyhow;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use chrono::{Duration as ChronoDuration, Utc};
use tracing::info;

use crate::{db::Db, iss, nasa, noaa};

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
}

impl AppState {
    pub fn new(client: reqwest::Client, db: Db, ml_url: String) -> Self {
        Self {
            client,
            db: Arc::new(Mutex::new(db)),
            ml_url,
            cache: Arc::new(Mutex::new(HashMap::new())),
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
        .with_state(state)
}

// ── NASA handlers ─────────────────────────────────────────────────────────────

async fn get_apod(State(s): State<AppState>) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "apod", Duration::from_secs(3600), || async {
        let apod = nasa::fetch_apod(&s.client).await?;
        info!("APOD: {}", apod.date);
        lock_db(&s.db).await.insert_apod(&apod)?;
        Ok(serde_json::to_value(apod)?)
    })
    .await
}

async fn get_neo(State(s): State<AppState>) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "neo", Duration::from_secs(1800), || async {
        let today = Utc::now().date_naive();
        let start = today.format("%Y-%m-%d").to_string();
        let end = (today + ChronoDuration::days(7)).format("%Y-%m-%d").to_string();
        let feed = nasa::fetch_neo_feed(&s.client, &start, &end).await?;
        info!("NEO: {} objects ({} to {})", feed.element_count, start, end);
        let fetched_at = Utc::now().timestamp();
        {
            let db = lock_db(&s.db).await;
            for neos in feed.near_earth_objects.values() {
                for neo in neos {
                    for approach in &neo.close_approach_data {
                        db.insert_neo(neo, approach, fetched_at)?;
                    }
                }
            }
        }
        Ok(serde_json::to_value(feed)?)
    })
    .await
}

async fn get_epic(State(s): State<AppState>) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "epic", Duration::from_secs(3600), || async {
        let images = nasa::fetch_epic(&s.client).await?;
        info!("EPIC: {} images", images.len());
        {
            let db = lock_db(&s.db).await;
            for img in &images {
                db.insert_epic_image(img)?;
            }
        }
        Ok(serde_json::to_value(images)?)
    })
    .await
}

async fn get_exoplanets(State(s): State<AppState>) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "exoplanets", Duration::from_secs(86400), || async {
        let planets = nasa::fetch_exoplanets(&s.client).await?;
        info!("Exoplanets: {}", planets.len());
        {
            let db = lock_db(&s.db).await;
            for p in &planets {
                db.insert_exoplanet(p)?;
            }
        }
        Ok(serde_json::to_value(planets)?)
    })
    .await
}

// ── NOAA handlers ─────────────────────────────────────────────────────────────

async fn get_kp(State(s): State<AppState>) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "kp", Duration::from_secs(60), || async {
        let records = noaa::fetch_kp(&s.client).await?;
        info!("Kp: {} records", records.len());
        {
            let db = lock_db(&s.db).await;
            for r in &records {
                db.insert_kp(r)?;
            }
        }
        Ok(serde_json::to_value(records)?)
    })
    .await
}

async fn get_solar_wind(State(s): State<AppState>) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "solar-wind", Duration::from_secs(60), || async {
        let records = noaa::fetch_solar_wind(&s.client).await?;
        info!("Solar wind: {} records", records.len());
        {
            let db = lock_db(&s.db).await;
            db.begin()?;
            let result = records.iter().try_for_each(|r| db.insert_solar_wind(r));
            match result {
                Ok(()) => db.commit()?,
                Err(e) => {
                    db.rollback();
                    return Err(e.into());
                }
            }
        }
        Ok(serde_json::to_value(records)?)
    })
    .await
}

async fn get_xray(State(s): State<AppState>) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "xray", Duration::from_secs(120), || async {
        let records = noaa::fetch_xray(&s.client).await?;
        info!("X-ray: {} records", records.len());
        {
            let db = lock_db(&s.db).await;
            db.begin()?;
            let result = records.iter().try_for_each(|r| db.insert_xray(r));
            match result {
                Ok(()) => db.commit()?,
                Err(e) => {
                    db.rollback();
                    return Err(e.into());
                }
            }
        }
        Ok(serde_json::to_value(records)?)
    })
    .await
}

async fn get_alerts(State(s): State<AppState>) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "alerts", Duration::from_secs(300), || async {
        let alerts = noaa::fetch_alerts(&s.client).await?;
        info!("Alerts: {}", alerts.len());
        {
            let db = lock_db(&s.db).await;
            for a in &alerts {
                db.insert_alert(a)?;
            }
        }
        Ok(serde_json::to_value(alerts)?)
    })
    .await
}

// ── ISS handler ───────────────────────────────────────────────────────────────

async fn get_iss(State(s): State<AppState>) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "iss", Duration::from_secs(5), || async {
        let pos = iss::fetch_iss_position(&s.client).await?;
        info!(
            "ISS: lat={:.4} lon={:.4} alt={:.1}km",
            pos.latitude, pos.longitude, pos.altitude
        );
        lock_db(&s.db).await.insert_iss_position(&pos)?;
        Ok(serde_json::to_value(pos)?)
    })
    .await
}

// ── ML forecast handler ───────────────────────────────────────────────────────

async fn get_kp_forecast(State(s): State<AppState>) -> Result<impl IntoResponse, AppError> {
    cached(&s.cache, "kp-forecast", Duration::from_secs(180), || async {
        let readings = lock_db(&s.db).await.get_recent_kp(7)?;

        if readings.is_empty() {
            return Err(anyhow!("no Kp data in database — call /api/kp first").into());
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

        Ok(payload)
    })
    .await
}
