use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::fetch::{Fetched, PollOutcome};

#[derive(Error, Debug)]
pub enum NoaaError {
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("upstream record is missing required field `{field}`")]
    MissingField { field: &'static str },
    #[error("upstream field `{field}` is not a number")]
    UnparseableField { field: &'static str },
    #[error("upstream payload is not valid json: {0}")]
    Json(#[from] serde_json::Error),
}

const SWPC: &str = "https://services.swpc.noaa.gov";

/// Replaces the bare `NaN`, `Infinity` and `-Infinity` value tokens that NOAA
/// emits with `null`, leaving everything else byte for byte alone.
///
/// RFC 8259 has no such literals, so `serde_json` rejects the whole document and
/// reqwest reports it as "error decoding response body". On 2026-08-10 eight
/// `NaN` samples in `rtsw_wind_1m.json` threw away all 3524 entries on every
/// poll for three hours; the solar wind table simply stopped.
///
/// A `NaN` sample means the instrument produced no valid reading for that field
/// at that minute, which is exactly what a nullable column is for. It is not a
/// zero, which would be a real measurement, and it is not cause to drop the
/// whole row, whose other fields are usually good. Turning it into `null` lets
/// the existing handling do the right thing: `parse_val` yields `None`, and the
/// column stores NULL.
///
/// This is deliberately not the same as an absent field. `parse_imf` still
/// treats a missing key as an error, because that means the schema moved, and
/// the silent freeze that caused is the reason it is strict.
///
/// String contents are skipped, so a station named `"NaN"` survives untouched.
fn nulls_for_nonstandard_numbers(input: &str) -> std::borrow::Cow<'_, str> {
    // Cheap reject: if none of the literals appear at all, hand back the input.
    if !input.contains("NaN") && !input.contains("Infinity") {
        return std::borrow::Cow::Borrowed(input);
    }

    // Byte oriented on purpose. Copying byte by byte keeps multi-byte UTF-8
    // inside strings intact; rebuilding it as `char` would mangle it.
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(input.len());
    let mut i = 0;
    let mut in_string = false;

    while i < bytes.len() {
        let b = bytes[i];
        if in_string {
            // Copy an escape pair whole, so a \" does not read as the closing
            // quote and drop us out of the string early.
            if b == b'\\' && i + 1 < bytes.len() {
                out.extend_from_slice(&bytes[i..i + 2]);
                i += 2;
                continue;
            }
            if b == b'"' {
                in_string = false;
            }
            out.push(b);
            i += 1;
            continue;
        }
        if b == b'"' {
            in_string = true;
            out.push(b);
            i += 1;
            continue;
        }
        // Outside a string these can only be value tokens, never field names.
        let rest = &bytes[i..];
        if rest.starts_with(b"NaN") {
            out.extend_from_slice(b"null");
            i += 3;
            continue;
        }
        if rest.starts_with(b"-Infinity") {
            out.extend_from_slice(b"null");
            i += 9;
            continue;
        }
        if rest.starts_with(b"Infinity") {
            out.extend_from_slice(b"null");
            i += 8;
            continue;
        }
        out.push(b);
        i += 1;
    }

    // Bytes are copied whole, so this cannot fail; falling back to the input
    // rather than asserting means a surprise here surfaces as a parse error
    // instead of a panic.
    match String::from_utf8(out) {
        Ok(s) => std::borrow::Cow::Owned(s),
        Err(_) => std::borrow::Cow::Borrowed(input),
    }
}

/// Fetches a NOAA JSON feed that is parsed value by value, tolerating the
/// non-standard number literals NOAA emits.
async fn fetch_lenient_json(
    client: &Client,
    url: String,
) -> Result<Vec<serde_json::Value>, NoaaError> {
    let text = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    Ok(serde_json::from_str(&nulls_for_nonstandard_numbers(&text))?)
}

/// Fetches a feed of typed records, one record at a time.
///
/// These feeds used to go straight through `.json::<Vec<T>>()`, where a single
/// bad entry rejected the entire payload. That is how one `NaN` would take out a
/// whole poll, and solar wind proved NOAA does emit them.
///
/// Turning `NaN` into `null` is not enough on its own here, because these
/// records hold plain `f64` rather than `Option<f64>`: a null would fail as
/// "invalid type: null, expected f64" and lose the payload anyway. That is the
/// right outcome for the record though, not for its neighbours. A Kp reading
/// whose Kp is not a number carries nothing, so the record is dropped and the
/// rest of the batch is kept.
///
/// Nothing is dropped quietly. The counts go back as [`PollOutcome`], so a
/// handful of bad entries logs a WARN with the number dropped, and a schema
/// change that kills every record still logs an ERROR, which is what the hourly
/// poller check greps for.
///
/// This is deliberately more forgiving than `parse_imf`, which treats an absent
/// field as an error. An absent field means the schema moved under us; an
/// unparseable one usually means a bad sample.
async fn fetch_record_list<T: serde::de::DeserializeOwned>(
    client: &Client,
    url: String,
) -> Result<Fetched<T>, NoaaError> {
    let items = fetch_lenient_json(client, url).await?;
    let received = items.len();
    let records: Vec<T> = items
        .into_iter()
        .filter_map(|item| serde_json::from_value(item).ok())
        .collect();
    Ok(Fetched::lossy(records, received))
}

// ── Kp index (1-minute) ───────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct KpRecord {
    pub time_tag: String,
    pub kp_index: i32,
    pub estimated_kp: f64,
}

pub async fn fetch_kp(client: &Client) -> Result<Fetched<KpRecord>, NoaaError> {
    fetch_record_list(client, format!("{SWPC}/json/planetary_k_index_1m.json")).await
}

// ── Kp index (3-hour official) ────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct Kp3hRecord {
    pub time_tag: String,
    #[serde(rename = "Kp")]
    pub kp: f64,
}

pub async fn fetch_kp_3h(client: &Client) -> Result<Fetched<Kp3hRecord>, NoaaError> {
    fetch_record_list(
        client,
        format!("{SWPC}/products/noaa-planetary-k-index.json"),
    )
    .await
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
pub async fn fetch_solar_wind(client: &Client) -> Result<Fetched<SolarWindRecord>, NoaaError> {
    let items = fetch_lenient_json(client, format!("{SWPC}/json/rtsw/rtsw_wind_1m.json")).await?;

    // Held before the filter_map consumes the vector, so a feed that changes
    // shape reports "sent 1440, kept 0" instead of a silent zero.
    let received = items.len();
    let records: Vec<SolarWindRecord> = items
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

    Ok(Fetched::lossy(records, received))
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

pub async fn fetch_xray(client: &Client) -> Result<Fetched<XRayRecord>, NoaaError> {
    fetch_record_list(client, format!("{SWPC}/json/goes/primary/xrays-1-day.json")).await
}

// ── Space weather alerts ──────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct SpaceWeatherAlert {
    pub product_id: String,
    pub issue_datetime: String,
    pub message: String,
}

pub async fn fetch_alerts(client: &Client) -> Result<Fetched<SpaceWeatherAlert>, NoaaError> {
    fetch_record_list(client, format!("{SWPC}/products/alerts.json")).await
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
pub async fn fetch_imf(client: &Client) -> Result<Fetched<ImfRecord>, NoaaError> {
    let items = fetch_lenient_json(client, format!("{SWPC}/json/rtsw/rtsw_mag_1m.json")).await?;

    // `parse_imf` returns one record per entry or fails the whole batch, so it
    // cannot drop a row quietly. That makes the count strict: zero records can
    // only mean an empty payload, never a parser that swallowed the feed.
    let records = parse_imf(items)?;
    let outcome = PollOutcome::strict(records.len());
    Ok(Fetched {
        items: records,
        outcome,
    })
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
fn numeric_field(item: &serde_json::Value, field: &'static str) -> Result<Option<f64>, NoaaError> {
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
pub async fn fetch_dst(client: &Client) -> Result<Fetched<DstRecord>, NoaaError> {
    let items = fetch_lenient_json(client, format!("{SWPC}/products/kyoto-dst.json")).await?;

    let received = items.len();
    let records: Vec<DstRecord> = items
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

    Ok(Fetched::lossy(records, received))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape that broke solar wind: a bare NaN sample among good rows.
    /// Every entry was discarded for three hours because of eight of these.
    #[test]
    fn nan_samples_become_null_and_the_payload_still_parses() {
        let raw = r#"[{"time_tag":"2026-08-10T23:45:00","proton_speed":368.72,"proton_density":1.01},
                      {"time_tag":"2026-08-10T23:46:00","proton_speed":NaN,"proton_density":1.02}]"#;
        assert!(
            serde_json::from_str::<Vec<serde_json::Value>>(raw).is_err(),
            "precondition: serde_json must reject the raw feed"
        );

        let cleaned = nulls_for_nonstandard_numbers(raw);
        let items: Vec<serde_json::Value> = serde_json::from_str(&cleaned).unwrap();
        assert_eq!(
            items.len(),
            2,
            "the good row must survive alongside the bad one"
        );
        assert!(items[1]["proton_speed"].is_null(), "NaN must become null");
        // Not zero. A zero speed is a real measurement and would be a lie.
        assert_ne!(items[1]["proton_speed"], serde_json::json!(0));
        // The rest of the row is intact, which is why the row is kept.
        assert_eq!(items[1]["proton_density"], serde_json::json!(1.02));
    }

    /// The typed feeds hold plain `f64`, so a NaN turned into null still fails
    /// that record. It must fail only that record.
    #[test]
    fn one_bad_typed_record_does_not_take_the_batch_with_it() {
        let raw = r#"[{"time_tag":"2026-08-10T23:45:00","kp_index":2,"estimated_kp":2.33},
                      {"time_tag":"2026-08-10T23:46:00","kp_index":3,"estimated_kp":NaN},
                      {"time_tag":"2026-08-10T23:47:00","kp_index":4,"estimated_kp":4.10}]"#;
        let items: Vec<serde_json::Value> =
            serde_json::from_str(&nulls_for_nonstandard_numbers(raw)).unwrap();
        let received = items.len();
        let kept: Vec<KpRecord> = items
            .into_iter()
            .filter_map(|i| serde_json::from_value(i).ok())
            .collect();

        assert_eq!(received, 3);
        assert_eq!(kept.len(), 2, "only the NaN record should be dropped");
        assert_eq!(kept[0].estimated_kp, 2.33);
        assert_eq!(kept[1].estimated_kp, 4.10);
        // And the drop is visible rather than silent.
        assert_eq!(
            crate::fetch::PollOutcome::lossy(received, kept.len()),
            crate::fetch::PollOutcome::Parsed {
                received: 3,
                kept: 2
            }
        );
    }

    /// A feed whose shape changed kills every record, which must still be loud.
    /// `log_poll` turns this exact value into an ERROR line, which is what the
    /// hourly poller check greps for.
    #[test]
    fn a_typed_feed_that_loses_every_record_reports_it_as_such() {
        let raw = r#"[{"time_tag":"2026-08-10T23:45:00","kp_index":2,"estimated_kp":NaN},
                      {"time_tag":"2026-08-10T23:46:00","kp_index":3,"estimated_kp":NaN}]"#;
        let items: Vec<serde_json::Value> =
            serde_json::from_str(&nulls_for_nonstandard_numbers(raw)).unwrap();
        let received = items.len();
        let kept: Vec<KpRecord> = items
            .into_iter()
            .filter_map(|i| serde_json::from_value(i).ok())
            .collect();

        assert_eq!(kept.len(), 0);
        assert_eq!(
            crate::fetch::PollOutcome::lossy(received, kept.len()),
            crate::fetch::PollOutcome::Parsed {
                received: 2,
                kept: 0
            },
            "must not collapse to EmptyPayload; rows were sent and all were lost"
        );
    }

    /// xray carries two f64 fields, so it is worth checking the same holds where
    /// more than one field can go bad.
    #[test]
    fn a_nan_in_either_xray_flux_drops_only_that_reading() {
        let raw = r#"[{"time_tag":"2026-08-10T23:45:00Z","satellite":18,"flux":1.2e-6,"observed_flux":1.3e-6,"energy":"0.1-0.8nm"},
                      {"time_tag":"2026-08-10T23:46:00Z","satellite":18,"flux":NaN,"observed_flux":1.4e-6,"energy":"0.1-0.8nm"},
                      {"time_tag":"2026-08-10T23:47:00Z","satellite":18,"flux":1.5e-6,"observed_flux":NaN,"energy":"0.1-0.8nm"}]"#;
        let items: Vec<serde_json::Value> =
            serde_json::from_str(&nulls_for_nonstandard_numbers(raw)).unwrap();
        let kept: Vec<XRayRecord> = items
            .into_iter()
            .filter_map(|i| serde_json::from_value(i).ok())
            .collect();
        assert_eq!(kept.len(), 1, "both NaN readings drop, the good one stays");
        assert_eq!(kept[0].time_tag, "2026-08-10T23:45:00Z");
    }

    #[test]
    fn a_string_containing_nan_is_left_alone() {
        // The reason this is a scanner and not a text replace.
        let raw = r#"{"source":"NaN detector","note":"he said \"NaN\" twice","v":NaN}"#;
        let cleaned = nulls_for_nonstandard_numbers(raw);
        let v: serde_json::Value = serde_json::from_str(&cleaned).unwrap();
        assert_eq!(v["source"], serde_json::json!("NaN detector"));
        assert_eq!(v["note"], serde_json::json!("he said \"NaN\" twice"));
        assert!(v["v"].is_null());
    }

    #[test]
    fn infinities_become_null_too() {
        let raw = r#"{"a":Infinity,"b":-Infinity,"c":1.5}"#;
        let v: serde_json::Value =
            serde_json::from_str(&nulls_for_nonstandard_numbers(raw)).unwrap();
        assert!(v["a"].is_null());
        assert!(
            v["b"].is_null(),
            "-Infinity must not leave a stray minus sign"
        );
        assert_eq!(v["c"], serde_json::json!(1.5));
    }

    #[test]
    fn a_clean_payload_is_returned_untouched() {
        let raw = r#"[{"time_tag":"2026-08-10T23:45:00","bt":14.71}]"#;
        assert!(matches!(
            nulls_for_nonstandard_numbers(raw),
            std::borrow::Cow::Borrowed(_)
        ));
    }

    #[test]
    fn non_ascii_inside_strings_survives() {
        // A byte-by-byte copy is required here; rebuilding through `char` mangles it.
        let raw =
            r#"{"station":"Kiruna Sverige aao","v":NaN}"#.replace("aao", "\u{e5}\u{e4}\u{f6}");
        let v: serde_json::Value =
            serde_json::from_str(&nulls_for_nonstandard_numbers(&raw)).unwrap();
        assert_eq!(
            v["station"],
            serde_json::json!("Kiruna Sverige \u{e5}\u{e4}\u{f6}")
        );
        assert!(v["v"].is_null());
    }

    /// An absent field stays an error. A NaN is a missing sample; a missing key
    /// means the schema moved, which is what froze the imf table for forty days.
    #[test]
    fn a_null_sample_is_none_but_an_absent_field_is_still_an_error() {
        let with_null =
            serde_json::json!({"time_tag":"2026-08-10T23:45:00","bz_gsm":null,"bt":1.0});
        let parsed = parse_imf(vec![with_null]).unwrap();
        assert_eq!(parsed.len(), 1);
        assert!(parsed[0].bz_gsm.is_none());

        let missing = serde_json::json!({"time_tag":"2026-08-10T23:45:00","bt":1.0});
        assert!(
            parse_imf(vec![missing]).is_err(),
            "an absent field must still fail"
        );
    }

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
            [
                "time_tag", "bx_gsm", "by_gsm", "bz_gsm", "lon_gsm", "lat_gsm", "bt"
            ],
            [
                "2026-06-30 18:40:00.000",
                "1.92",
                "-14.50",
                "1.01",
                "0",
                "0",
                "14.71"
            ]
        ]);
        let items = legacy.as_array().expect("array").clone();
        assert!(parse_imf(items).is_err());
    }
}
