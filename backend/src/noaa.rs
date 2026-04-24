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

// ── IMF Bz (DSCOVR magnetometer) ─────────────────────────────────────────────

#[derive(Debug)]
pub struct ImfRecord {
    pub time_tag: String,
    pub bz_gsm:   Option<f64>,
    pub bt:        Option<f64>,
}

/// Parses the 2-D array format: row 0 is the header, rows 1+ are data.
/// Columns: [time_tag, bx_gsm, by_gsm, bz_gsm, lon_gsm, lat_gsm, bt]
/// Note: NOAA encodes numeric values as JSON strings (e.g. "-4.74"), not numbers.
pub async fn fetch_imf(client: &Client) -> Result<Vec<ImfRecord>, NoaaError> {
    let rows: Vec<Vec<serde_json::Value>> = client
        .get(format!("{SWPC}/products/solar-wind/mag-1-day.json"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let records = rows
        .into_iter()
        .skip(1)
        .filter_map(|row| {
            let time_tag = row.first()?.as_str()?.to_owned();
            let bz_gsm   = parse_val(row.get(3)?);
            let bt        = parse_val(row.get(6)?);
            Some(ImfRecord { time_tag, bz_gsm, bt })
        })
        .collect();

    Ok(records)
}

/// Handles both JSON numbers and JSON strings containing a float (NOAA uses strings).
fn parse_val(v: &serde_json::Value) -> Option<f64> {
    v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

// ── Dst index (Kyoto WDC via NOAA SWPC proxy) ────────────────────────────────

#[derive(Debug)]
pub struct DstRecord {
    pub time_tag: String,
    pub dst_nt:   Option<i32>,
}

/// Parses the array-of-objects format: [{"time_tag":"...","dst":-45}, ...].
pub async fn fetch_dst(client: &Client) -> Result<Vec<DstRecord>, NoaaError> {
    let items: Vec<serde_json::Value> = client
        .get(format!("{SWPC}/products/kyoto-dst.json"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let records = items
        .into_iter()
        .filter_map(|item| {
            let time_tag = item.get("time_tag")?.as_str()?.to_owned();
            // NOAA emits integers but guard against floats.
            let dst_nt = item.get("dst").and_then(|v| v.as_f64()).map(|v| v.round() as i32);
            Some(DstRecord { time_tag, dst_nt })
        })
        .collect();

    Ok(records)
}
