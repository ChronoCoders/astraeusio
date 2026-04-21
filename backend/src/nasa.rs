#![allow(dead_code)]

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum NasaError {
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("missing env var: {0}")]
    Env(#[from] std::env::VarError),
}

fn api_key() -> Result<String, NasaError> {
    Ok(std::env::var("NASA_API_KEY")?)
}

// ── APOD ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct Apod {
    pub date: String,
    pub title: String,
    pub explanation: String,
    pub url: String,
    pub media_type: String,
    pub hdurl: Option<String>,
}

pub async fn fetch_apod(client: &Client) -> Result<Apod, NasaError> {
    let key = api_key()?;
    Ok(client
        .get(format!("https://api.nasa.gov/planetary/apod?api_key={key}"))
        .send()
        .await?
        .error_for_status()?
        .json::<Apod>()
        .await?)
}

// ── NeoWs ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct NeoFeed {
    pub element_count: u32,
    pub near_earth_objects: HashMap<String, Vec<NearEarthObject>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NearEarthObject {
    pub id: String,
    pub name: String,
    pub is_potentially_hazardous_asteroid: bool,
    pub estimated_diameter: EstimatedDiameter,
    pub close_approach_data: Vec<CloseApproach>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EstimatedDiameter {
    pub kilometers: DiameterRange,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiameterRange {
    pub estimated_diameter_min: f64,
    pub estimated_diameter_max: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CloseApproach {
    pub close_approach_date: String,
    pub relative_velocity: RelativeVelocity,
    pub miss_distance: MissDistance,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RelativeVelocity {
    pub kilometers_per_hour: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MissDistance {
    pub kilometers: String,
}

pub async fn fetch_neo_feed(
    client: &Client,
    start_date: &str,
    end_date: &str,
) -> Result<NeoFeed, NasaError> {
    let key = api_key()?;
    Ok(client
        .get(format!(
            "https://api.nasa.gov/neo/rest/v1/feed\
             ?start_date={start_date}&end_date={end_date}&api_key={key}"
        ))
        .send()
        .await?
        .error_for_status()?
        .json::<NeoFeed>()
        .await?)
}

// ── EPIC ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct EpicImage {
    pub identifier: String,
    pub caption: String,
    pub image: String,
    pub date: String,
    pub centroid_coordinates: CentroidCoordinates,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CentroidCoordinates {
    pub lat: f64,
    pub lon: f64,
}

pub async fn fetch_epic(client: &Client) -> Result<Vec<EpicImage>, NasaError> {
    let key = api_key()?;
    Ok(client
        .get(format!("https://api.nasa.gov/EPIC/api/natural?api_key={key}"))
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<EpicImage>>()
        .await?)
}

// ── Exoplanet Archive (TAP) ───────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct Exoplanet {
    pub pl_name: String,
    pub hostname: String,
    pub pl_orbper: Option<f64>,
    pub pl_rade: Option<f64>,
    pub pl_masse: Option<f64>,
    pub disc_year: Option<i32>,
}

pub async fn fetch_exoplanets(client: &Client) -> Result<Vec<Exoplanet>, NasaError> {
    let url = "https://exoplanetarchive.ipac.caltech.edu/TAP/sync\
               ?query=select+pl_name,hostname,pl_orbper,pl_rade,pl_masse,disc_year\
               +from+ps+where+default_flag=1+order+by+disc_year+desc\
               &format=json&maxrec=100";
    Ok(client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<Exoplanet>>()
        .await?)
}
