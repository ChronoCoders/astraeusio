#![allow(dead_code)]

use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum IssError {
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),
}

// wheretheiss.at provides altitude + velocity; Open Notify (api.open-notify.org)
// only returns lat/lon, so this source is used to satisfy the full position spec.
#[derive(Debug, Serialize, Deserialize)]
pub struct IssPosition {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: f64,
    pub velocity: f64,
    pub timestamp: i64,
}

pub async fn fetch_iss_position(client: &Client) -> Result<IssPosition, IssError> {
    Ok(client
        .get("https://api.wheretheiss.at/v1/satellites/25544")
        .send()
        .await?
        .error_for_status()?
        .json::<IssPosition>()
        .await?)
}
