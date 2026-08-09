use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum NoaaError {
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("upstream record is missing required field `{field}`")]
    MissingField { field: &'static str },
    #[error("upstream field `{field}` is not a number")]
    UnparseableField { field: &'static str },
}

const SWPC: &str = "https://services.swpc.noaa.gov";

// ── Kp index (1-minute) ───────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct KpRecord {
    pub time_tag: String,
    pub kp_index: i32,
    pub estimated_kp: f64,
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

// ── Kp index (3-hour official) ────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct Kp3hRecord {
    pub time_tag: String,
    #[serde(rename = "Kp")]
    pub kp: f64,
}

pub async fn fetch_kp_3h(client: &Client) -> Result<Vec<Kp3hRecord>, NoaaError> {
    Ok(client
        .get(format!("{SWPC}/products/noaa-planetary-k-index.json"))
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<Kp3hRecord>>()
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

/// NOAA sometimes encodes numeric fields as JSON strings (same as IMF feed),
/// so we parse via Value and coerce manually to tolerate both formats.
pub async fn fetch_solar_wind(client: &Client) -> Result<Vec<SolarWindRecord>, NoaaError> {
    let items: Vec<serde_json::Value> = client
        .get(format!("{SWPC}/json/rtsw/rtsw_wind_1m.json"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let records = items
        .into_iter()
        .filter_map(|item| {
            let time_tag = item.get("time_tag")?.as_str()?.to_owned();
            Some(SolarWindRecord {
                time_tag,
                proton_speed: item.get("proton_speed").and_then(parse_val),
                proton_density: item.get("proton_density").and_then(parse_val),
                proton_temperature: item.get("proton_temperature").and_then(parse_val),
            })
        })
        .collect();

    Ok(records)
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
    pub bz_gsm: Option<f64>,
    pub bt: Option<f64>,
}

/// Reads the rtsw magnetometer feed, an array of objects keyed by field name.
///
/// The previous feed at products/solar-wind/mag-1-day.json was a positional
/// 2-D array. NOAA retired it and it returned 404 for forty days while the
/// poller logged an error every minute and wrote nothing, so the table sat
/// frozen. This reads the rtsw family, the same shape fetch_solar_wind uses.
pub async fn fetch_imf(client: &Client) -> Result<Vec<ImfRecord>, NoaaError> {
    let items: Vec<serde_json::Value> = client
        .get(format!("{SWPC}/json/rtsw/rtsw_mag_1m.json"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    parse_imf(items)
}

/// Maps the raw feed to records. Separate from the request so it can be tested.
///
/// An absent field means the upstream schema moved again and is an error, so a
/// second silent freeze is not possible. A field present but null is a missing
/// sample and stays None, which is what the nullable columns are for.
fn parse_imf(items: Vec<serde_json::Value>) -> Result<Vec<ImfRecord>, NoaaError> {
    let mut records = Vec::with_capacity(items.len());
    for item in items {
        let time_tag = item
            .get("time_tag")
            .ok_or(NoaaError::MissingField { field: "time_tag" })?
            .as_str()
            .ok_or(NoaaError::MissingField { field: "time_tag" })?
            .to_owned();
        records.push(ImfRecord {
            time_tag,
            bz_gsm: numeric_field(&item, "bz_gsm")?,
            bt: numeric_field(&item, "bt")?,
        });
    }
    Ok(records)
}

/// Reads one numeric field by name. Absent is an error, null is None.
fn numeric_field(
    item: &serde_json::Value,
    field: &'static str,
) -> Result<Option<f64>, NoaaError> {
    match item.get(field) {
        None => Err(NoaaError::MissingField { field }),
        Some(v) if v.is_null() => Ok(None),
        Some(v) => parse_val(v)
            .map(Some)
            .ok_or(NoaaError::UnparseableField { field }),
    }
}

/// Handles both JSON numbers and JSON strings containing a float (NOAA uses strings).
fn parse_val(v: &serde_json::Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

// ── Dst index (Kyoto WDC via NOAA SWPC proxy) ────────────────────────────────

#[derive(Debug)]
pub struct DstRecord {
    pub time_tag: String,
    pub dst_nt: Option<i32>,
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
            let dst_nt = item
                .get("dst")
                .and_then(|v| v.as_f64())
                .map(|v| v.round() as i32);
            Some(DstRecord { time_tag, dst_nt })
        })
        .collect();

    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One real record from rtsw_mag_1m.json, trimmed to the fields that matter
    /// plus a few neighbours so field-name lookup is exercised, not position.
    fn sample() -> serde_json::Value {
        serde_json::json!({
            "time_tag": "2026-08-09T02:38:02",
            "active": true,
            "source": "IMAP",
            "bt": 14.71,
            "bx_gse": 1.92,
            "by_gse": -14.50,
            "bz_gse": 1.04,
            "bx_gsm": 1.92,
            "by_gsm": -14.50,
            "bz_gsm": 1.01,
            "sample_size": 60
        })
    }

    #[test]
    fn imf_reads_fields_by_name_not_position() {
        let records = parse_imf(vec![sample()]).expect("well formed record parses");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].time_tag, "2026-08-09T02:38:02");
        // bz_gsm and bz_gse differ, so a positional read would pick the wrong one.
        assert_eq!(records[0].bz_gsm, Some(1.01));
        assert_eq!(records[0].bt, Some(14.71));
    }

    #[test]
    fn imf_accepts_numbers_encoded_as_strings() {
        let mut item = sample();
        item["bt"] = serde_json::json!("14.71");
        item["bz_gsm"] = serde_json::json!("-3.5");
        let records = parse_imf(vec![item]).expect("string encoded numbers parse");
        assert_eq!(records[0].bt, Some(14.71));
        assert_eq!(records[0].bz_gsm, Some(-3.5));
    }

    #[test]
    fn imf_treats_an_absent_field_as_an_error() {
        for field in ["time_tag", "bz_gsm", "bt"] {
            let mut item = sample();
            item.as_object_mut().expect("object").remove(field);
            let err = parse_imf(vec![item]).expect_err("a missing field must fail the fetch");
            match err {
                NoaaError::MissingField { field: got } => assert_eq!(got, field),
                other => panic!("expected MissingField for {field}, got {other:?}"),
            }
        }
    }

    #[test]
    fn imf_keeps_a_null_measurement_as_none() {
        let mut item = sample();
        item["bz_gsm"] = serde_json::Value::Null;
        let records = parse_imf(vec![item]).expect("an explicit null is a missing sample");
        assert_eq!(records[0].bz_gsm, None);
        assert_eq!(records[0].bt, Some(14.71));
    }

    #[test]
    fn imf_rejects_a_field_that_is_present_but_not_numeric() {
        let mut item = sample();
        item["bt"] = serde_json::json!("n/a");
        let err = parse_imf(vec![item]).expect_err("a non numeric value must fail the fetch");
        assert!(matches!(err, NoaaError::UnparseableField { field: "bt" }));
    }

    /// The retired feed was a 2-D array with a header row. Handing that shape to
    /// the new parser must fail loudly rather than yield records.
    #[test]
    fn imf_rejects_the_retired_positional_format() {
        let legacy = serde_json::json!([
            ["time_tag", "bx_gsm", "by_gsm", "bz_gsm", "lon_gsm", "lat_gsm", "bt"],
            ["2026-06-30 18:40:00.000", "1.92", "-14.50", "1.01", "0", "0", "14.71"]
        ]);
        let items = legacy.as_array().expect("array").clone();
        assert!(parse_imf(items).is_err());
    }
}
