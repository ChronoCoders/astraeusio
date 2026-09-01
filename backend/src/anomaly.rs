use tracing::warn;

use crate::db::{DbError, Store};
use crate::db_writer::{DbWriterHandle, WriteCmd};

// Kp >= 5.0 = G1 storm, >= 8.0 = G4 severe
const KP_WARNING_E2: i64 = 500;
const KP_CRITICAL_E2: i64 = 800;

// Solar wind speed > 700 km/s (stored as km/s * 10)
const WIND_WARNING_E1: i64 = 7_000;
const WIND_CRITICAL_E1: i64 = 9_000;

// 1 Lunar Distance = 384,400 km; stored as km * 1000
const ONE_LD_SCALED: i64 = 384_400_000;
const HALF_LD_SCALED: i64 = 192_200_000;

// X-ray flux * 1e12: M-class >= 1e7, X-class >= 1e8
const XRAY_M_E12: i64 = 10_000_000;
const XRAY_X_E12: i64 = 100_000_000;

// ML forecast: same Kp thresholds
const FORECAST_WARNING_E2: i64 = 500;
const FORECAST_CRITICAL_E2: i64 = 800;

// ── Pure threshold logic (unit-tested below) ────────────────────────────────────

/// Severity for a raw Kp reading (scaled ×100), or `None` if below G1 (Kp 5).
fn kp_severity(kp_e2: i64) -> Option<&'static str> {
    if kp_e2 >= KP_CRITICAL_E2 {
        Some("critical")
    } else if kp_e2 >= KP_WARNING_E2 {
        Some("warning")
    } else {
        None
    }
}

/// Severity for a solar-wind speed reading (scaled ×10), or `None` if below threshold.
fn wind_severity(speed_e1: i64) -> Option<&'static str> {
    if speed_e1 >= WIND_CRITICAL_E1 {
        Some("critical")
    } else if speed_e1 >= WIND_WARNING_E1 {
        Some("warning")
    } else {
        None
    }
}

/// Format an X-ray flux reading (W/m²) in human-readable NOAA notation
/// (e.g. "1.55 × 10⁻⁵") instead of `1.55e-5`.
fn format_xray_flux(flux_w_m2: f64) -> String {
    if flux_w_m2 <= 0.0 {
        return "0".to_string();
    }
    let exp = flux_w_m2.log10().floor() as i32;
    let mantissa = flux_w_m2 / 10f64.powi(exp);
    let sup = superscript_signed(exp);
    format!("{mantissa:.2} × 10{sup}")
}

fn superscript_signed(n: i32) -> String {
    let mut out = String::new();
    if n < 0 {
        out.push('⁻');
    }
    for ch in n.abs().to_string().chars() {
        let digit = match ch {
            '0' => '⁰',
            '1' => '¹',
            '2' => '²',
            '3' => '³',
            '4' => '⁴',
            '5' => '⁵',
            '6' => '⁶',
            '7' => '⁷',
            '8' => '⁸',
            '9' => '⁹',
            _ => ch,
        };
        out.push(digit);
    }
    out
}

/// `(severity, class)` for an X-ray flux reading (scaled ×1e12), or `None` below M-class.
fn xray_severity(flux_e12: i64) -> Option<(&'static str, &'static str)> {
    if flux_e12 >= XRAY_X_E12 {
        Some(("critical", "X"))
    } else if flux_e12 >= XRAY_M_E12 {
        Some(("warning", "M"))
    } else {
        None
    }
}

/// Severity for a close-approaching NEO. Callers pre-filter to within 1 LD, so
/// this only distinguishes critical (< 0.5 LD) from warning.
fn neo_severity(dist_scaled: i64) -> &'static str {
    if dist_scaled < HALF_LD_SCALED {
        "critical"
    } else {
        "warning"
    }
}

/// Severity for an ML forecast Kp (scaled ×100), or `None` if no storm predicted.
fn forecast_severity(kp_e2: i64) -> Option<&'static str> {
    if kp_e2 >= FORECAST_CRITICAL_E2 {
        Some("critical")
    } else if kp_e2 >= FORECAST_WARNING_E2 {
        Some("warning")
    } else {
        None
    }
}

pub fn detect_and_store(db: &Store, writer: &DbWriterHandle) -> Result<(), DbError> {
    check_kp(db, writer)?;
    check_solar_wind(db, writer)?;
    check_xray(db, writer)?;
    check_neo(db, writer)?;
    check_ml_forecast(db, writer)?;
    check_custom_rules(db, writer)?;
    Ok(())
}

fn check_kp(db: &Store, writer: &DbWriterHandle) -> Result<(), DbError> {
    if let Some((time_tag, _, kp_e2)) = db.latest_kp_raw()?
        && let Some(severity) = kp_severity(kp_e2)
    {
        let kp = kp_e2 as f64 / 100.0;
        let msg = format!("Kp index {kp:.1} - geomagnetic storm in progress");
        writer.fire(WriteCmd::Anomaly {
            anomaly_type: "kp_storm".to_string(),
            source_ref: time_tag,
            severity: severity.to_string(),
            message: msg.clone(),
            user_email: None,
        });
        warn!(anomaly = "kp_storm", kp, severity, "{msg}");
    }
    Ok(())
}

fn check_solar_wind(db: &Store, writer: &DbWriterHandle) -> Result<(), DbError> {
    if let Some((time_tag, _, speed_e1)) = db.latest_solar_wind_speed_raw()?
        && let Some(severity) = wind_severity(speed_e1)
    {
        let speed = speed_e1 as f64 / 10.0;
        let msg = format!("Solar wind speed {speed:.0} km/s exceeds threshold");
        writer.fire(WriteCmd::Anomaly {
            anomaly_type: "solar_wind_speed".to_string(),
            source_ref: time_tag,
            severity: severity.to_string(),
            message: msg.clone(),
            user_email: None,
        });
        warn!(anomaly = "solar_wind_speed", speed, severity, "{msg}");
    }
    Ok(())
}

fn check_xray(db: &Store, writer: &DbWriterHandle) -> Result<(), DbError> {
    // Scan the last 3 hours for the peak reading so flares that have already
    // peaked and decayed are still caught on the next detection cycle.
    let since = now() - 3 * 3600;
    if let Some((time_tag, flux_e12)) = db.xray_peak_recent(since)?
        && let Some((severity, class)) = xray_severity(flux_e12)
    {
        // Standard NOAA/SWPC notation: M1.5 = 1.5 × 10⁻⁵ W/m², X2.3 = 2.3 × 10⁻⁴ W/m².
        let (class_base, exponent) = if class == "X" {
            (1e8_f64, "⁻⁴")
        } else {
            (1e7_f64, "⁻⁵")
        };
        let magnitude = flux_e12 as f64 / class_base;
        let msg = format!(
            "{class}{magnitude:.1} X-ray flare detected ({magnitude:.2} × 10{exponent} W/m²)"
        );
        writer.fire(WriteCmd::Anomaly {
            anomaly_type: "xray_flare".to_string(),
            source_ref: time_tag,
            severity: severity.to_string(),
            message: msg.clone(),
            user_email: None,
        });
        warn!(anomaly = "xray_flare", class, severity, "{msg}");
    }
    Ok(())
}

fn check_neo(db: &Store, writer: &DbWriterHandle) -> Result<(), DbError> {
    let since = now() - 7 * 24 * 3600;
    for (id, date, dist_scaled) in db.neo_close_approaches_raw(ONE_LD_SCALED, since)? {
        let severity = neo_severity(dist_scaled);
        let dist_km = dist_scaled as f64 / 1_000.0;
        let dist_ld = dist_km / 384_400.0;
        let msg = format!("Asteroid {id} passes {dist_ld:.3} LD ({dist_km:.0} km) on {date}");
        let source_ref = format!("{id}:{date}");
        writer.fire(WriteCmd::Anomaly {
            anomaly_type: "asteroid_close".to_string(),
            source_ref: source_ref.clone(),
            severity: severity.to_string(),
            message: msg.clone(),
            user_email: None,
        });
        warn!(anomaly = "asteroid_close", %id, %date, dist_ld, severity, "{msg}");
    }
    Ok(())
}

fn check_ml_forecast(db: &Store, writer: &DbWriterHandle) -> Result<(), DbError> {
    let since = now() - 24 * 3600;
    if let Some((ts, kp_e2)) = db.get_kp_forecast_max_recent(since)?
        && let Some(severity) = forecast_severity(kp_e2)
    {
        let kp = kp_e2 as f64 / 100.0;
        let source_ref = ts.to_string();
        let msg = format!("ML model forecasts Kp {kp:.1} - storm predicted within 3 hours");
        writer.fire(WriteCmd::Anomaly {
            anomaly_type: "ml_forecast_storm".to_string(),
            source_ref,
            severity: severity.to_string(),
            message: msg.clone(),
            user_email: None,
        });
        warn!(anomaly = "ml_forecast_storm", kp, severity, "{msg}");
    }
    Ok(())
}


// ── Metric scales ─────────────────────────────────────────────────────────────

/// What each metric is stored at, so a rule threshold can be held in the same
/// units as the reading it is compared against.
///
/// Thresholds used to be DOUBLE, and the comparison converted the stored integer
/// to f64 first, so a reading exactly on the boundary could land either side of
/// it depending on which decimal fractions happened to be representable. Both
/// sides are integers now and the boundary is exact.
pub struct MetricScale {
    pub metric: &'static str,
    /// Multiplier from the value a caller supplies to the stored integer.
    pub scale: f64,
    /// Smallest step the metric can express, for the error message.
    pub step: &'static str,
}

pub const METRIC_SCALES: [MetricScale; 5] = [
    MetricScale { metric: "kp", scale: 100.0, step: "0.01" },
    MetricScale { metric: "solar_wind_speed", scale: 10.0, step: "0.1 km/s" },
    MetricScale { metric: "xray_flux", scale: 1e12, step: "0.000000000001 W/m2" },
    MetricScale { metric: "dst", scale: 1.0, step: "1 nT" },
    MetricScale { metric: "imf_bz", scale: 100.0, step: "0.01 nT" },
];

pub fn metric_scale(metric: &str) -> Option<&'static MetricScale> {
    METRIC_SCALES.iter().find(|m| m.metric == metric)
}

#[derive(Debug, PartialEq, Eq)]
pub enum ThresholdError {
    UnknownMetric,
    NotFinite,
    OutOfRange,
    /// The caller gave more precision than the metric stores.
    TooPrecise { step: &'static str },
}

/// Converts a caller supplied threshold into the metric's stored units.
///
/// Nothing meaningful is rounded. A value carrying more precision than the
/// metric can hold is rejected rather than quietly moved, because silently
/// shifting a threshold changes when someone's alert fires and they would never
/// know. The only rounding absorbs IEEE-754 representation error: scaling
/// -10.05 by 100 lands on -1005.0000000000001, which is the same number.
///
/// The tolerance has to be absolute rather than relative. Representation error
/// grows with magnitude, but a meaningful digit is always worth at least a
/// fraction of one stored unit whatever the scale, so a purely relative
/// tolerance gets looser exactly where it needs to stay tight: at the xray_flux
/// scale of 1e12, a relative 1e-6 would allow 12 whole units of slack and
/// swallow a real 0.1 error. This is mostly absolute, with a small relative
/// term for the largest values, capped so it can never reach the 0.1 that marks
/// genuine extra precision.
pub fn scale_threshold(metric: &str, value: f64) -> Result<i64, ThresholdError> {
    let Some(m) = metric_scale(metric) else {
        return Err(ThresholdError::UnknownMetric);
    };
    if !value.is_finite() {
        return Err(ThresholdError::NotFinite);
    }
    let scaled = value * m.scale;
    if !scaled.is_finite() || scaled.abs() >= 9.0e18 {
        return Err(ThresholdError::OutOfRange);
    }
    let nearest = scaled.round();
    let tolerance = (1e-6 + nearest.abs() * 1e-12).min(1e-3);
    if (scaled - nearest).abs() > tolerance {
        return Err(ThresholdError::TooPrecise { step: m.step });
    }
    Ok(nearest as i64)
}

/// Back to the caller's units, for display.
pub fn unscale_threshold(metric: &str, scaled: i64) -> f64 {
    match metric_scale(metric) {
        Some(m) => scaled as f64 / m.scale,
        None => scaled as f64,
    }
}

fn check_custom_rules(db: &Store, writer: &DbWriterHandle) -> Result<(), DbError> {
    let rules = db.get_enabled_custom_rules()?;
    if rules.is_empty() {
        return Ok(());
    }
    let hour_bucket = now() / 3600;
    for rule in &rules {
        // Both sides stay in the metric's stored units, so a reading exactly
        // on the threshold compares exactly.
        let raw = match rule.metric.as_str() {
            "kp" => db.latest_kp_raw()?.map(|(_, _, v)| v),
            "solar_wind_speed" => db.latest_solar_wind_speed_raw()?.map(|(_, _, v)| v),
            "xray_flux" => db.latest_xray_flux_raw()?.map(|(_, v)| v),
            "dst" => db.latest_dst_raw()?.map(|(_, v)| v),
            "imf_bz" => db.latest_imf_bz_raw()?.map(|(_, v)| v),
            _ => None,
        };
        let Some(scaled_val) = raw else { continue };
        let val = unscale_threshold(&rule.metric, scaled_val);
        let triggered = match rule.operator.as_str() {
            "gt" => scaled_val > rule.threshold_scaled,
            "lt" => scaled_val < rule.threshold_scaled,
            "gte" => scaled_val >= rule.threshold_scaled,
            "lte" => scaled_val <= rule.threshold_scaled,
            _ => false,
        };
        if !triggered {
            continue;
        }
        let op_label = match rule.operator.as_str() {
            "gt" => ">",
            "lt" => "<",
            "gte" => "≥",
            "lte" => "≤",
            _ => "?",
        };
        let metric_str = match rule.metric.as_str() {
            "kp" => format!("Kp {val:.2}"),
            "solar_wind_speed" => format!("Solar wind {val:.0} km/s"),
            "xray_flux" => format!("X-ray {} W/m²", format_xray_flux(val)),
            "dst" => format!("Dst {val:.0} nT"),
            "imf_bz" => format!("IMF Bz {val:.2} nT"),
            m => format!("{m} = {val:.3}"),
        };
        let msg = format!(
            "Custom rule '{}': {} {} {}",
            rule.name,
            metric_str,
            op_label,
            unscale_threshold(&rule.metric, rule.threshold_scaled)
        );
        writer.fire(WriteCmd::Anomaly {
            anomaly_type: format!("custom:{}", rule.id),
            source_ref: format!("{}:{}", rule.id, hour_bucket),
            severity: rule.severity.clone(),
            message: msg,
            // Owned by the account whose rule fired. Before this column, the
            // rule's name and threshold went into a feed every authenticated
            // caller could read.
            user_email: Some(rule.user_email.clone()),
        });
    }
    Ok(())
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kp_severity_thresholds() {
        assert_eq!(kp_severity(0), None);
        assert_eq!(kp_severity(499), None); // just below G1
        assert_eq!(kp_severity(500), Some("warning")); // Kp 5.0 = G1
        assert_eq!(kp_severity(799), Some("warning"));
        assert_eq!(kp_severity(800), Some("critical")); // Kp 8.0 = G4
        assert_eq!(kp_severity(900), Some("critical"));
    }

    #[test]
    fn wind_severity_thresholds() {
        assert_eq!(wind_severity(0), None);
        assert_eq!(wind_severity(6_999), None); // 699.9 km/s
        assert_eq!(wind_severity(7_000), Some("warning")); // 700 km/s
        assert_eq!(wind_severity(8_999), Some("warning"));
        assert_eq!(wind_severity(9_000), Some("critical")); // 900 km/s
    }

    #[test]
    fn xray_severity_classes() {
        assert_eq!(xray_severity(0), None);
        assert_eq!(xray_severity(9_999_999), None); // below M (1e-5 W/m²)
        assert_eq!(xray_severity(10_000_000), Some(("warning", "M")));
        assert_eq!(xray_severity(99_999_999), Some(("warning", "M")));
        assert_eq!(xray_severity(100_000_000), Some(("critical", "X"))); // 1e-4 W/m²
    }

    #[test]
    fn neo_severity_boundary() {
        // Caller pre-filters to ≤ 1 LD, so input is always within range.
        assert_eq!(neo_severity(HALF_LD_SCALED - 1), "critical");
        assert_eq!(neo_severity(HALF_LD_SCALED), "warning"); // exactly 0.5 LD
        assert_eq!(neo_severity(ONE_LD_SCALED), "warning");
    }

    #[test]
    fn forecast_severity_thresholds() {
        assert_eq!(forecast_severity(499), None);
        assert_eq!(forecast_severity(500), Some("warning"));
        assert_eq!(forecast_severity(800), Some("critical"));
    }
}

#[cfg(test)]
mod threshold_tests {
    use super::*;

    /// Realistic thresholds convert exactly, in the metric's own units.
    #[test]
    fn thresholds_convert_to_the_metrics_own_units() {
        for (metric, value, expected) in [
            ("kp", 5.0, 500),
            ("kp", 5.67, 567),
            ("kp", 4.33, 433),
            ("solar_wind_speed", 700.0, 7_000),
            ("solar_wind_speed", 700.5, 7_005),
            ("dst", -50.0, -50),
            ("imf_bz", -10.0, -1_000),
            // Scaling -10.05 by 100 lands on -1005.0000000000001 in f64. That
            // is the same number, and must not be mistaken for extra precision.
            ("imf_bz", -10.05, -1_005),
            // M-class and X-class, the two thresholds anyone actually sets.
            ("xray_flux", 1e-5, 10_000_000),
            ("xray_flux", 1e-4, 100_000_000),
            ("xray_flux", 5e-6, 5_000_000),
        ] {
            assert_eq!(
                scale_threshold(metric, value),
                Ok(expected),
                "{metric} {value}"
            );
            let back = unscale_threshold(metric, expected);
            assert!(
                (back - value).abs() <= value.abs() * 1e-9 + f64::EPSILON,
                "{metric} {value} came back as {back}"
            );
        }
    }

    /// More precision than the metric holds is refused, not quietly moved.
    /// Shifting someone's threshold changes when their alert fires.
    #[test]
    fn extra_precision_is_refused_rather_than_rounded() {
        assert_eq!(
            scale_threshold("kp", 5.005),
            Err(ThresholdError::TooPrecise { step: "0.01" })
        );
        assert_eq!(
            scale_threshold("dst", -50.5),
            Err(ThresholdError::TooPrecise { step: "1 nT" })
        );
        assert_eq!(
            scale_threshold("solar_wind_speed", 700.55),
            Err(ThresholdError::TooPrecise { step: "0.1 km/s" })
        );
        // xray_flux is the one where this bites: 1e12 leaves a lot of room, but
        // a value below a millionth of a millionth still has nowhere to go.
        assert!(matches!(
            scale_threshold("xray_flux", 1.23456789e-5),
            Err(ThresholdError::TooPrecise { .. })
        ));
        assert!(matches!(
            scale_threshold("xray_flux", 1e-15),
            Err(ThresholdError::TooPrecise { .. })
        ));
    }

    #[test]
    fn unusable_values_are_refused() {
        assert_eq!(scale_threshold("kp", f64::NAN), Err(ThresholdError::NotFinite));
        assert_eq!(scale_threshold("kp", f64::INFINITY), Err(ThresholdError::NotFinite));
        assert_eq!(scale_threshold("kp", 1e30), Err(ThresholdError::OutOfRange));
        assert_eq!(scale_threshold("nonsense", 1.0), Err(ThresholdError::UnknownMetric));
    }

    /// Every metric a rule may name must have a scale, or its threshold would
    /// be stored in units nothing compares against.
    #[test]
    fn every_rule_metric_has_a_scale() {
        for metric in ["kp", "solar_wind_speed", "xray_flux", "dst", "imf_bz"] {
            assert!(metric_scale(metric).is_some(), "{metric} has no scale");
        }
        assert_eq!(METRIC_SCALES.len(), 5);
    }

    /// The reason for the change: a reading exactly on the threshold. As f64 the
    /// comparison went through a division that cannot represent every decimal,
    /// so the boundary was not reliable. As integers it is exact.
    #[test]
    fn a_reading_exactly_on_the_threshold_compares_exactly() {
        // Kp 5.67 stored as 567. A rule at 5.67 with gte must fire, gt must not.
        // Mirrors the operator match in check_custom_rules.
        let fires = |op: &str, v: i64, t: i64| match op {
            "gt" => v > t,
            "lt" => v < t,
            "gte" => v >= t,
            "lte" => v <= t,
            _ => false,
        };

        let stored: i64 = 567;
        let threshold = scale_threshold("kp", 5.67).expect("scale");
        assert_eq!(stored, threshold);
        assert!(fires("gte", stored, threshold), "gte fires on the boundary");
        assert!(!fires("gt", stored, threshold), "gt does not fire on the boundary");
        assert!(fires("lte", stored, threshold), "lte fires on the boundary");
        assert!(!fires("lt", stored, threshold), "lt does not fire on the boundary");

        // One step either side behaves as expected.
        assert!(fires("gt", stored + 1, threshold));
        assert!(fires("lt", stored - 1, threshold));
    }
}
