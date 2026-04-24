#![allow(dead_code)]

use reqwest::Client;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StarlinkError {
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),
}

#[derive(Debug)]
pub struct StarlinkSat {
    pub norad_id:  i32,
    pub name:      String,
    pub tle_line1: String,
    pub tle_line2: String,
}

/// Fetches 3-line TLE text format and parses each name/line1/line2 triplet.
/// FORMAT=json from Celestrak returns GP elements without TLE strings;
/// FORMAT=tle returns the classic format that contains the actual TLE lines.
///
/// Celestrak refreshes data every 2 hours and returns 304 or 403 when no new
/// data is available since the last download. Both are treated as soft
/// "no update needed" — the poller skips the insert, keeping existing DB rows.
pub async fn fetch_starlink(client: &Client) -> Result<Vec<StarlinkSat>, StarlinkError> {
    let resp = client
        .get("https://celestrak.org/NORAD/elements/gp.php?GROUP=starlink&FORMAT=tle")
        .send()
        .await?;

    let status = resp.status();
    if status == reqwest::StatusCode::NOT_MODIFIED
        || status == reqwest::StatusCode::FORBIDDEN
    {
        return Ok(Vec::new());
    }

    let text = resp.error_for_status()?.text().await?;

    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();

    let mut sats = Vec::new();
    let mut i = 0;
    while i + 2 < lines.len() {
        let name  = lines[i];
        let line1 = lines[i + 1];
        let line2 = lines[i + 2];

        // Validate TLE line identifiers before consuming the triplet.
        if !line1.starts_with('1') || !line2.starts_with('2') {
            i += 1;
            continue;
        }

        // NORAD catalog number occupies columns 3-7 (0-indexed: 2..7).
        if let Ok(norad_id) = line1[2..7].trim().parse::<i32>() {
            sats.push(StarlinkSat {
                norad_id,
                name:      name.to_owned(),
                tle_line1: line1.to_owned(),
                tle_line2: line2.to_owned(),
            });
        }

        i += 3;
    }

    Ok(sats)
}
