use tracing::warn;

use crate::db::{Store, DbError};
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

pub fn detect_and_store(db: &Store, writer: &DbWriterHandle) -> Result<(), DbError> {
    check_kp(db, writer)?;
    check_solar_wind(db, writer)?;
    check_xray(db, writer)?;
    check_neo(db, writer)?;
    check_ml_forecast(db, writer)?;
    Ok(())
}

fn check_kp(db: &Store, writer: &DbWriterHandle) -> Result<(), DbError> {
    if let Some((time_tag, kp_e2)) = db.latest_kp_raw()?
        && kp_e2 >= KP_WARNING_E2
    {
        let severity = if kp_e2 >= KP_CRITICAL_E2 {
            "critical"
        } else {
            "warning"
        };
        let kp = kp_e2 as f64 / 100.0;
        let msg = format!("Kp index {kp:.1} — geomagnetic storm in progress");
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
        && speed_e1 >= WIND_WARNING_E1
    {
        let severity = if speed_e1 >= WIND_CRITICAL_E1 {
            "critical"
        } else {
            "warning"
        };
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
    if let Some((time_tag, flux_e12)) = db.latest_xray_flux_raw()?
        && flux_e12 >= XRAY_M_E12
    {
        let (severity, class) = if flux_e12 >= XRAY_X_E12 {
            ("critical", "X")
        } else {
            ("warning", "M")
        };
        let flux = flux_e12 as f64 / 1e12;
        let msg = format!("X-ray class {class} flare detected (flux: {flux:.2e} W/m²)");
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
        let severity = if dist_scaled < HALF_LD_SCALED {
            "critical"
        } else {
            "warning"
        };
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
        && kp_e2 >= FORECAST_WARNING_E2
    {
        let severity = if kp_e2 >= FORECAST_CRITICAL_E2 {
            "critical"
        } else {
            "warning"
        };
        let kp = kp_e2 as f64 / 100.0;
        let source_ref = ts.to_string();
        let msg = format!("ML model forecasts Kp {kp:.1} — storm predicted within 3 hours");
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

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
