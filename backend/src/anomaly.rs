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
    if let Some((time_tag, kp_e2)) = db.latest_kp_raw()?
        && let Some(severity) = kp_severity(kp_e2)
    {
        let kp = kp_e2 as f64 / 100.0;
        let msg = format!("Kp index {kp:.1} - geomagnetic storm in progress");
        writer.fire(WriteCmd::Anomaly {
            anomaly_type: "kp_storm".to_string(),
            source_ref: time_tag,
            severity: severity.to_string(),
            message: msg.clone(),
        });
        warn!(anomaly = "kp_storm", kp, severity, "{msg}");
    }
    Ok(())
}

fn check_solar_wind(db: &Store, writer: &DbWriterHandle) -> Result<(), DbError> {
    if let Some((time_tag, speed_e1)) = db.latest_solar_wind_speed_raw()?
        && let Some(severity) = wind_severity(speed_e1)
    {
        let speed = speed_e1 as f64 / 10.0;
        let msg = format!("Solar wind speed {speed:.0} km/s exceeds threshold");
        writer.fire(WriteCmd::Anomaly {
            anomaly_type: "solar_wind_speed".to_string(),
            source_ref: time_tag,
            severity: severity.to_string(),
            message: msg.clone(),
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
        });
        warn!(anomaly = "ml_forecast_storm", kp, severity, "{msg}");
    }
    Ok(())
}

fn check_custom_rules(db: &Store, writer: &DbWriterHandle) -> Result<(), DbError> {
    let rules = db.get_enabled_custom_rules()?;
    if rules.is_empty() {
        return Ok(());
    }
    let hour_bucket = now() / 3600;
    for rule in &rules {
        let raw = match rule.metric.as_str() {
            "kp" => db.latest_kp_raw()?.map(|(_, v)| v as f64 / 100.0),
            "solar_wind_speed" => db
                .latest_solar_wind_speed_raw()?
                .map(|(_, v)| v as f64 / 10.0),
            "xray_flux" => db.latest_xray_flux_raw()?.map(|(_, v)| v as f64 / 1e12),
            "dst" => db.latest_dst_raw()?.map(|(_, v)| v as f64),
            "imf_bz" => db.latest_imf_bz_raw()?.map(|(_, v)| v as f64 / 100.0),
            _ => None,
        };
        let Some(val) = raw else { continue };
        let triggered = match rule.operator.as_str() {
            "gt" => val > rule.threshold,
            "lt" => val < rule.threshold,
            "gte" => val >= rule.threshold,
            "lte" => val <= rule.threshold,
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
            rule.name, metric_str, op_label, rule.threshold
        );
        writer.fire(WriteCmd::Anomaly {
            anomaly_type: format!("custom:{}", rule.id),
            source_ref: format!("{}:{}", rule.id, hour_bucket),
            severity: rule.severity.clone(),
            message: msg,
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
