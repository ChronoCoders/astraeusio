#![allow(dead_code)]

use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum NoaaError {
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),
}

const SWPC: &str = "https://services.swpc.noaa.gov";

// ── Kp index ──────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct KpRecord {
    pub time_tag: String,
    pub kp_index: i32,
    pub estimated_kp: f64,
    pub kp: String,
}

pub async fn fetch_kp(client: &Client) -> Result<Vec<KpRecord>, NoaaError> {
    Ok(client
        .get(format!("{SWPC}/json/planetary_k_index_1m.json"))
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<KpRecord>>()
        .await?)
}

// ── Solar wind ────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct SolarWindRecord {
    pub time_tag: String,
    pub proton_speed: Option<f64>,
    pub proton_density: Option<f64>,
    pub proton_temperature: Option<f64>,
}

pub async fn fetch_solar_wind(client: &Client) -> Result<Vec<SolarWindRecord>, NoaaError> {
    Ok(client
        .get(format!("{SWPC}/json/rtsw/rtsw_wind_1m.json"))
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<SolarWindRecord>>()
        .await?)
}

// ── X-ray flux ────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct XRayRecord {
    pub time_tag: String,
    pub satellite: i32,
    pub flux: f64,
    pub observed_flux: f64,
    pub energy: String,
}

pub async fn fetch_xray(client: &Client) -> Result<Vec<XRayRecord>, NoaaError> {
    Ok(client
        .get(format!("{SWPC}/json/goes/primary/xrays-1-day.json"))
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<XRayRecord>>()
        .await?)
}

// ── Space weather alerts ──────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct SpaceWeatherAlert {
    pub product_id: String,
    pub issue_datetime: String,
    pub message: String,
}

pub async fn fetch_alerts(client: &Client) -> Result<Vec<SpaceWeatherAlert>, NoaaError> {
    Ok(client
        .get(format!("{SWPC}/products/alerts.json"))
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<SpaceWeatherAlert>>()
        .await?)
}
