use std::sync::Arc;
use tokio::sync::{Mutex, MutexGuard};

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use chrono::{Duration, Utc};
use tracing::info;

use crate::{db::Db, iss, nasa, noaa};

// ── State ─────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AppState {
    pub client: reqwest::Client,
    pub db: Arc<Mutex<Db>>,
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
        .with_state(state)
}

// ── NASA handlers ─────────────────────────────────────────────────────────────

async fn get_apod(State(s): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let apod = nasa::fetch_apod(&s.client).await?;
    info!("APOD: {}", apod.date);
    lock_db(&s.db).await.insert_apod(&apod)?;
    Ok(Json(apod))
}

async fn get_neo(State(s): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let today = Utc::now().date_naive();
    let start = today.format("%Y-%m-%d").to_string();
    let end = (today + Duration::days(7)).format("%Y-%m-%d").to_string();

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
    Ok(Json(feed))
}

async fn get_epic(State(s): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let images = nasa::fetch_epic(&s.client).await?;
    info!("EPIC: {} images", images.len());
    {
        let db = lock_db(&s.db).await;
        for img in &images {
            db.insert_epic_image(img)?;
        }
    }
    Ok(Json(images))
}

async fn get_exoplanets(State(s): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let planets = nasa::fetch_exoplanets(&s.client).await?;
    info!("Exoplanets: {}", planets.len());
    {
        let db = lock_db(&s.db).await;
        for p in &planets {
            db.insert_exoplanet(p)?;
        }
    }
    Ok(Json(planets))
}

// ── NOAA handlers ─────────────────────────────────────────────────────────────

async fn get_kp(State(s): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let records = noaa::fetch_kp(&s.client).await?;
    info!("Kp: {} records", records.len());
    {
        let db = lock_db(&s.db).await;
        for r in &records {
            db.insert_kp(r)?;
        }
    }
    Ok(Json(records))
}

async fn get_solar_wind(State(s): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let records = noaa::fetch_solar_wind(&s.client).await?;
    info!("Solar wind: {} records", records.len());
    {
        let db = lock_db(&s.db).await;
        db.begin()?;
        let result = records.iter().try_for_each(|r| db.insert_solar_wind(r));
        match result {
            Ok(()) => db.commit()?,
            Err(e) => { db.rollback(); return Err(e.into()); }
        }
    }
    Ok(Json(records))
}

async fn get_xray(State(s): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let records = noaa::fetch_xray(&s.client).await?;
    info!("X-ray: {} records", records.len());
    {
        let db = lock_db(&s.db).await;
        db.begin()?;
        let result = records.iter().try_for_each(|r| db.insert_xray(r));
        match result {
            Ok(()) => db.commit()?,
            Err(e) => { db.rollback(); return Err(e.into()); }
        }
    }
    Ok(Json(records))
}

async fn get_alerts(State(s): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let alerts = noaa::fetch_alerts(&s.client).await?;
    info!("Alerts: {}", alerts.len());
    {
        let db = lock_db(&s.db).await;
        for a in &alerts {
            db.insert_alert(a)?;
        }
    }
    Ok(Json(alerts))
}

// ── ISS handler ───────────────────────────────────────────────────────────────

async fn get_iss(State(s): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let pos = iss::fetch_iss_position(&s.client).await?;
    info!("ISS: lat={:.4} lon={:.4} alt={:.1}km", pos.latitude, pos.longitude, pos.altitude);
    lock_db(&s.db).await.insert_iss_position(&pos)?;
    Ok(Json(pos))
}
