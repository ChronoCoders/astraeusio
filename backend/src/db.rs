use duckdb::{Connection, params};
use thiserror::Error;

use crate::{
    iss::IssPosition,
    nasa::{Apod, EpicImage, Exoplanet, NeoFeed},
    noaa::{DstRecord, ImfRecord, Kp3hRecord, KpRecord, SolarWindRecord, SpaceWeatherAlert, XRayRecord},
    starlink::StarlinkSat,
};

#[derive(Error, Debug)]
pub enum DbError {
    #[error("database error: {0}")]
    Duckdb(#[from] duckdb::Error),
    #[error("parse error for field '{field}': {value}")]
    Parse { field: &'static str, value: String },
    #[error("email already registered")]
    EmailTaken,
    #[error("api key not found")]
    KeyNotFound,
}

// ── Schema ────────────────────────────────────────────────────────────────────

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS apod (
    date        TEXT   NOT NULL PRIMARY KEY,
    title       TEXT   NOT NULL,
    explanation TEXT   NOT NULL,
    url         TEXT   NOT NULL,
    media_type  TEXT   NOT NULL,
    hdurl       TEXT,
    fetched_at  BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS neo (
    id                  TEXT    NOT NULL,
    close_approach_date TEXT    NOT NULL,
    name                TEXT    NOT NULL,
    is_hazardous        BOOLEAN NOT NULL,
    diameter_min_m      BIGINT  NOT NULL,
    diameter_max_m      BIGINT  NOT NULL,
    velocity_m_per_h    BIGINT  NOT NULL,
    miss_distance_m     BIGINT  NOT NULL,
    fetched_at          BIGINT  NOT NULL,
    PRIMARY KEY (id, close_approach_date)
);

CREATE TABLE IF NOT EXISTS epic (
    identifier      TEXT   NOT NULL PRIMARY KEY,
    caption         TEXT   NOT NULL,
    image           TEXT   NOT NULL,
    date            TEXT   NOT NULL,
    centroid_lat_e6 BIGINT NOT NULL,
    centroid_lon_e6 BIGINT NOT NULL,
    fetched_at      BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS exoplanet (
    pl_name           TEXT    NOT NULL PRIMARY KEY,
    hostname          TEXT    NOT NULL,
    orbital_period_md BIGINT,
    radius_me3        BIGINT,
    mass_me3          BIGINT,
    disc_year         INTEGER,
    fetched_at        BIGINT  NOT NULL
);

CREATE TABLE IF NOT EXISTS kp (
    time_tag      TEXT   NOT NULL PRIMARY KEY,
    kp_index      INTEGER NOT NULL,
    estimated_kp_e2 BIGINT NOT NULL,
    fetched_at    BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS solar_wind (
    time_tag   TEXT   NOT NULL PRIMARY KEY,
    speed_e1   BIGINT,
    density_e2 BIGINT,
    temp_k     BIGINT,
    fetched_at BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS xray (
    time_tag          TEXT    NOT NULL,
    energy            TEXT    NOT NULL,
    satellite         INTEGER NOT NULL,
    flux_e12          BIGINT  NOT NULL,
    observed_flux_e12 BIGINT  NOT NULL,
    fetched_at        BIGINT  NOT NULL,
    PRIMARY KEY (time_tag, energy)
);

CREATE TABLE IF NOT EXISTS space_weather_alert (
    product_id     TEXT   NOT NULL,
    issue_datetime TEXT   NOT NULL,
    message        TEXT   NOT NULL,
    fetched_at     BIGINT NOT NULL,
    PRIMARY KEY (product_id, issue_datetime)
);

CREATE TABLE IF NOT EXISTS iss_position (
    ts           BIGINT NOT NULL PRIMARY KEY,
    lat_e6       BIGINT NOT NULL,
    lon_e6       BIGINT NOT NULL,
    altitude_m   BIGINT NOT NULL,
    velocity_m_h BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS users (
    email         TEXT   NOT NULL PRIMARY KEY,
    password_hash TEXT   NOT NULL,
    created_at    BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS kp_forecast (
    ts         BIGINT NOT NULL PRIMARY KEY,
    kp_e2      BIGINT NOT NULL,
    fetched_at BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS alerts_anomaly (
    anomaly_type TEXT   NOT NULL,
    source_ref   TEXT   NOT NULL,
    detected_at  BIGINT NOT NULL,
    severity     TEXT   NOT NULL,
    message      TEXT   NOT NULL,
    PRIMARY KEY (anomaly_type, source_ref)
);

CREATE TABLE IF NOT EXISTS starlink (
    norad_id   INTEGER NOT NULL PRIMARY KEY,
    name       TEXT    NOT NULL,
    tle_line1  TEXT    NOT NULL,
    tle_line2  TEXT    NOT NULL,
    fetched_at BIGINT  NOT NULL
);

CREATE TABLE IF NOT EXISTS imf (
    time_tag   TEXT   NOT NULL PRIMARY KEY,
    bz_e2      BIGINT,
    bt_e2      BIGINT,
    fetched_at BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS dst (
    time_tag   TEXT    NOT NULL PRIMARY KEY,
    dst_nt     INTEGER,
    fetched_at BIGINT  NOT NULL
);

CREATE TABLE IF NOT EXISTS kp_3h (
    time_tag   TEXT   NOT NULL PRIMARY KEY,
    kp_e2      BIGINT NOT NULL,
    fetched_at BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS api_keys (
    id            TEXT   NOT NULL PRIMARY KEY,
    user_email    TEXT   NOT NULL,
    key_hash      TEXT   NOT NULL UNIQUE,
    name          TEXT   NOT NULL,
    created_at    BIGINT NOT NULL,
    last_used_at  BIGINT,
    request_count BIGINT NOT NULL DEFAULT 0
);
";

// ── Db ────────────────────────────────────────────────────────────────────────

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open(path: &str) -> Result<Self, DbError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        // Migrate existing DBs that pre-date the plan column.
        if let Err(e) = conn.execute_batch(
            "ALTER TABLE users ADD COLUMN plan TEXT DEFAULT 'starter'",
        ) {
            let msg = e.to_string().to_lowercase();
            if !msg.contains("already exists") && !msg.contains("duplicate") {
                return Err(DbError::Duckdb(e));
            }
        }
        Ok(Self { conn })
    }

    pub fn begin(&self) -> Result<(), DbError> {
        self.conn.execute_batch("BEGIN")?;
        Ok(())
    }

    pub fn commit(&self) -> Result<(), DbError> {
        self.conn.execute_batch("COMMIT")?;
        Ok(())
    }

    pub fn rollback(&self) {
        let _ = self.conn.execute_batch("ROLLBACK");
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn scale(v: f64, factor: f64) -> i64 {
    (v * factor).round() as i64
}

fn scale_opt(v: Option<f64>, factor: f64) -> Option<i64> {
    v.map(|x| scale(x, factor))
}

fn parse_f64(field: &'static str, s: &str) -> Result<f64, DbError> {
    s.parse().map_err(|_| DbError::Parse {
        field,
        value: s.to_owned(),
    })
}

// ── NASA inserts ──────────────────────────────────────────────────────────────

impl Db {
    pub fn insert_apod(&self, a: &Apod) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO apod (date, title, explanation, url, media_type, hdurl, fetched_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (date) DO NOTHING",
            params![
                a.date,
                a.title,
                a.explanation,
                a.url,
                a.media_type,
                a.hdurl,
                now()
            ],
        )?;
        Ok(())
    }

    pub fn insert_epic_batch(&self, images: &[EpicImage]) -> Result<(), DbError> {
        if images.is_empty() {
            return Ok(());
        }
        self.begin()?;
        let result = (|| {
            let mut stmt = self.conn.prepare(
                "INSERT INTO epic
                 (identifier, caption, image, date,
                  centroid_lat_e6, centroid_lon_e6, fetched_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT (identifier) DO NOTHING",
            )?;
            for img in images {
                stmt.execute(params![
                    img.identifier,
                    img.caption,
                    img.image,
                    img.date,
                    scale(img.centroid_coordinates.lat, 1_000_000.0),
                    scale(img.centroid_coordinates.lon, 1_000_000.0),
                    now()
                ])?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => self.commit(),
            Err(e) => {
                self.rollback();
                Err(e)
            }
        }
    }

    pub fn insert_neo_batch(&self, feed: &NeoFeed, fetched_at: i64) -> Result<(), DbError> {
        self.begin()?;
        let result = (|| {
            let mut stmt = self.conn.prepare(
                "INSERT INTO neo
                 (id, close_approach_date, name, is_hazardous,
                  diameter_min_m, diameter_max_m,
                  velocity_m_per_h, miss_distance_m, fetched_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT (id, close_approach_date) DO NOTHING",
            )?;
            for neos in feed.near_earth_objects.values() {
                for neo in neos {
                    for approach in &neo.close_approach_data {
                        let vel = parse_f64(
                            "velocity_kmh",
                            &approach.relative_velocity.kilometers_per_hour,
                        )?;
                        let dist =
                            parse_f64("miss_distance_km", &approach.miss_distance.kilometers)?;
                        stmt.execute(params![
                            neo.id,
                            approach.close_approach_date,
                            neo.name,
                            neo.is_potentially_hazardous_asteroid,
                            scale(
                                neo.estimated_diameter.kilometers.estimated_diameter_min,
                                1_000.0
                            ),
                            scale(
                                neo.estimated_diameter.kilometers.estimated_diameter_max,
                                1_000.0
                            ),
                            scale(vel, 1_000.0),
                            scale(dist, 1_000.0),
                            fetched_at
                        ])?;
                    }
                }
            }
            Ok(())
        })();
        match result {
            Ok(()) => self.commit(),
            Err(e) => {
                self.rollback();
                Err(e)
            }
        }
    }

    pub fn insert_exoplanet_batch(&self, planets: &[Exoplanet]) -> Result<(), DbError> {
        if planets.is_empty() {
            return Ok(());
        }
        self.begin()?;
        let result = (|| {
            let mut stmt = self.conn.prepare(
                "INSERT INTO exoplanet
                 (pl_name, hostname, orbital_period_md,
                  radius_me3, mass_me3, disc_year, fetched_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT (pl_name) DO NOTHING",
            )?;
            for exo in planets {
                stmt.execute(params![
                    exo.pl_name,
                    exo.hostname,
                    scale_opt(exo.pl_orbper, 1_000.0),
                    scale_opt(exo.pl_rade, 1_000.0),
                    scale_opt(exo.pl_masse, 1_000.0),
                    exo.disc_year,
                    now()
                ])?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => self.commit(),
            Err(e) => {
                self.rollback();
                Err(e)
            }
        }
    }
}

// ── NOAA inserts ──────────────────────────────────────────────────────────────

impl Db {
    pub fn insert_kp_batch(&self, records: &[KpRecord]) -> Result<(), DbError> {
        if records.is_empty() {
            return Ok(());
        }
        let max_tag: Option<String> = self
            .conn
            .query_row("SELECT MAX(time_tag) FROM kp", [], |row| {
                row.get::<_, Option<String>>(0)
            })
            .unwrap_or(None);
        let to_insert: Vec<&KpRecord> = match &max_tag {
            Some(max) => records.iter().filter(|r| &r.time_tag > max).collect(),
            None => records.iter().collect(),
        };
        if to_insert.is_empty() {
            return Ok(());
        }
        self.begin()?;
        let result = (|| {
            let mut stmt = self.conn.prepare(
                "INSERT INTO kp (time_tag, kp_index, estimated_kp_e2, fetched_at)
                 VALUES (?, ?, ?, ?)
                 ON CONFLICT (time_tag) DO NOTHING",
            )?;
            for r in to_insert {
                stmt.execute(params![
                    r.time_tag,
                    r.kp_index,
                    scale(r.estimated_kp, 100.0),
                    now()
                ])?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => self.commit(),
            Err(e) => {
                self.rollback();
                Err(e)
            }
        }
    }

    pub fn insert_solar_wind_batch(&self, records: &[SolarWindRecord]) -> Result<(), DbError> {
        if records.is_empty() {
            return Ok(());
        }
        let max_tag: Option<String> = self
            .conn
            .query_row("SELECT MAX(time_tag) FROM solar_wind", [], |row| {
                row.get::<_, Option<String>>(0)
            })
            .unwrap_or(None);
        let to_insert: Vec<&SolarWindRecord> = match &max_tag {
            Some(max) => records.iter().filter(|r| &r.time_tag > max).collect(),
            None => records.iter().collect(),
        };
        if to_insert.is_empty() {
            return Ok(());
        }
        self.begin()?;
        let result = (|| {
            let mut stmt = self.conn.prepare(
                "INSERT INTO solar_wind (time_tag, speed_e1, density_e2, temp_k, fetched_at)
                 VALUES (?, ?, ?, ?, ?)
                 ON CONFLICT (time_tag) DO NOTHING",
            )?;
            for r in to_insert {
                stmt.execute(params![
                    r.time_tag,
                    scale_opt(r.proton_speed, 10.0),
                    scale_opt(r.proton_density, 100.0),
                    r.proton_temperature.map(|t| t.round() as i64),
                    now()
                ])?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => self.commit(),
            Err(e) => {
                self.rollback();
                Err(e)
            }
        }
    }

    pub fn insert_xray_batch(&self, records: &[XRayRecord]) -> Result<(), DbError> {
        if records.is_empty() {
            return Ok(());
        }
        let max_tag: Option<String> = self
            .conn
            .query_row("SELECT MAX(time_tag) FROM xray", [], |row| {
                row.get::<_, Option<String>>(0)
            })
            .unwrap_or(None);
        let to_insert: Vec<&XRayRecord> = match &max_tag {
            Some(max) => records.iter().filter(|r| &r.time_tag > max).collect(),
            None => records.iter().collect(),
        };
        if to_insert.is_empty() {
            return Ok(());
        }
        self.begin()?;
        let result = (|| {
            let mut stmt = self.conn.prepare(
                "INSERT INTO xray
                 (time_tag, energy, satellite, flux_e12, observed_flux_e12, fetched_at)
                 VALUES (?, ?, ?, ?, ?, ?)
                 ON CONFLICT (time_tag, energy) DO NOTHING",
            )?;
            for r in to_insert {
                stmt.execute(params![
                    r.time_tag,
                    r.energy,
                    r.satellite,
                    scale(r.flux, 1e12),
                    scale(r.observed_flux, 1e12),
                    now()
                ])?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => self.commit(),
            Err(e) => {
                self.rollback();
                Err(e)
            }
        }
    }

    pub fn insert_alerts_batch(&self, alerts: &[SpaceWeatherAlert]) -> Result<(), DbError> {
        if alerts.is_empty() {
            return Ok(());
        }
        self.begin()?;
        let result = (|| {
            let mut stmt = self.conn.prepare(
                "INSERT INTO space_weather_alert
                 (product_id, issue_datetime, message, fetched_at)
                 VALUES (?, ?, ?, ?)
                 ON CONFLICT (product_id, issue_datetime) DO NOTHING",
            )?;
            for a in alerts {
                stmt.execute(params![a.product_id, a.issue_datetime, a.message, now()])?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => self.commit(),
            Err(e) => {
                self.rollback();
                Err(e)
            }
        }
    }

    pub fn insert_imf_batch(&self, records: &[ImfRecord]) -> Result<(), DbError> {
        if records.is_empty() {
            return Ok(());
        }
        let max_tag: Option<String> = self
            .conn
            .query_row("SELECT MAX(time_tag) FROM imf", [], |row| {
                row.get::<_, Option<String>>(0)
            })
            .unwrap_or(None);
        let to_insert: Vec<&ImfRecord> = match &max_tag {
            Some(max) => records.iter().filter(|r| &r.time_tag > max).collect(),
            None => records.iter().collect(),
        };
        if to_insert.is_empty() {
            return Ok(());
        }
        self.begin()?;
        let result = (|| {
            let mut stmt = self.conn.prepare(
                "INSERT INTO imf (time_tag, bz_e2, bt_e2, fetched_at)
                 VALUES (?, ?, ?, ?)
                 ON CONFLICT (time_tag) DO NOTHING",
            )?;
            for r in to_insert {
                stmt.execute(params![
                    r.time_tag,
                    scale_opt(r.bz_gsm, 100.0),
                    scale_opt(r.bt, 100.0),
                    now()
                ])?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => self.commit(),
            Err(e) => {
                self.rollback();
                Err(e)
            }
        }
    }

    pub fn insert_dst_batch(&self, records: &[DstRecord]) -> Result<(), DbError> {
        if records.is_empty() {
            return Ok(());
        }
        let max_tag: Option<String> = self
            .conn
            .query_row("SELECT MAX(time_tag) FROM dst", [], |row| {
                row.get::<_, Option<String>>(0)
            })
            .unwrap_or(None);
        let to_insert: Vec<&DstRecord> = match &max_tag {
            Some(max) => records.iter().filter(|r| &r.time_tag > max).collect(),
            None => records.iter().collect(),
        };
        if to_insert.is_empty() {
            return Ok(());
        }
        self.begin()?;
        let result = (|| {
            let mut stmt = self.conn.prepare(
                "INSERT INTO dst (time_tag, dst_nt, fetched_at)
                 VALUES (?, ?, ?)
                 ON CONFLICT (time_tag) DO NOTHING",
            )?;
            for r in to_insert {
                stmt.execute(params![r.time_tag, r.dst_nt, now()])?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => self.commit(),
            Err(e) => {
                self.rollback();
                Err(e)
            }
        }
    }

    pub fn insert_kp_3h_batch(&self, records: &[Kp3hRecord]) -> Result<(), DbError> {
        if records.is_empty() {
            return Ok(());
        }
        self.begin()?;
        let result = (|| {
            let mut stmt = self.conn.prepare(
                "INSERT INTO kp_3h (time_tag, kp_e2, fetched_at)
                 VALUES (?, ?, ?)
                 ON CONFLICT (time_tag) DO UPDATE SET kp_e2 = excluded.kp_e2, fetched_at = excluded.fetched_at",
            )?;
            for r in records {
                stmt.execute(params![r.time_tag, scale(r.kp, 100.0), now()])?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => self.commit(),
            Err(e) => {
                self.rollback();
                Err(e)
            }
        }
    }
}

// ── NOAA queries ─────────────────────────────────────────────────────────────

impl Db {
    /// Returns the `n` most recent Kp readings ordered oldest-first.
    pub fn get_recent_kp(&self, n: usize) -> Result<Vec<f64>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT estimated_kp_e2 FROM kp ORDER BY time_tag DESC LIMIT ?")?;
        let rows: Vec<i64> = stmt
            .query_map([n as i64], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        // Reverse so values are oldest-first, de-scale back to Kp float.
        Ok(rows.into_iter().rev().map(|v| v as f64 / 100.0).collect())
    }

    pub fn get_kp_recent(&self) -> Result<serde_json::Value, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT time_tag, kp_index, estimated_kp_e2 FROM kp ORDER BY time_tag ASC LIMIT 1440",
        )?;
        let rows = stmt
            .query_map([], |row| {
                let time_tag: String = row.get(0)?;
                let kp_index: i32 = row.get(1)?;
                let estimated_kp_e2: i64 = row.get(2)?;
                Ok(serde_json::json!({
                    "time_tag": time_tag,
                    "kp_index": kp_index,
                    "estimated_kp": estimated_kp_e2 as f64 / 100.0,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(serde_json::Value::Array(rows))
    }

    /// Bucketed average Kp for the given time window. Bucket size adapts to range
    /// so charts always receive ≤ ~200 points regardless of period length.
    pub fn get_kp_range(&self, since_secs: i64) -> Result<serde_json::Value, DbError> {
        let cutoff = now() - since_secs;
        let bucket = if since_secs <= 86_400 { 900 } else if since_secs <= 604_800 { 3_600 } else { 21_600 };
        let sql = format!(
            "SELECT MIN(time_tag) as time_tag, CAST(AVG(estimated_kp_e2) AS BIGINT) as kp_e2 \
             FROM kp WHERE fetched_at > ? GROUP BY fetched_at / {bucket} ORDER BY time_tag ASC"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map([cutoff], |row| {
                let time_tag: String = row.get(0)?;
                let kp_e2: i64 = row.get(1)?;
                Ok(serde_json::json!({ "time_tag": time_tag, "estimated_kp": kp_e2 as f64 / 100.0 }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(serde_json::Value::Array(rows))
    }

    /// Bucketed average solar wind speed for the given time window.
    pub fn get_solar_wind_range(&self, since_secs: i64) -> Result<serde_json::Value, DbError> {
        let cutoff = now() - since_secs;
        let bucket = if since_secs <= 86_400 { 900 } else if since_secs <= 604_800 { 3_600 } else { 21_600 };
        let sql = format!(
            "SELECT MIN(time_tag) as time_tag, CAST(AVG(speed_e1) AS BIGINT) as speed_e1 \
             FROM solar_wind WHERE fetched_at > ? AND speed_e1 IS NOT NULL \
             GROUP BY fetched_at / {bucket} ORDER BY time_tag ASC"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map([cutoff], |row| {
                let time_tag: String = row.get(0)?;
                let speed_e1: i64 = row.get(1)?;
                Ok(serde_json::json!({ "time_tag": time_tag, "proton_speed": speed_e1 as f64 / 10.0 }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(serde_json::Value::Array(rows))
    }

    pub fn get_kp_3h_recent(&self) -> Result<serde_json::Value, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT time_tag, kp_e2 FROM kp_3h ORDER BY time_tag ASC LIMIT 240",
        )?;
        let rows = stmt
            .query_map([], |row| {
                let time_tag: String = row.get(0)?;
                let kp_e2: i64 = row.get(1)?;
                Ok(serde_json::json!({
                    "time_tag": time_tag,
                    "estimated_kp": kp_e2 as f64 / 100.0,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(serde_json::Value::Array(rows))
    }

    pub fn get_solar_wind_recent(&self) -> Result<serde_json::Value, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT time_tag, speed_e1, density_e2, temp_k FROM solar_wind \
             ORDER BY time_tag DESC LIMIT 1440",
        )?;
        let rows = stmt
            .query_map([], |row| {
                let time_tag: String = row.get(0)?;
                let speed_e1: Option<i64> = row.get(1)?;
                let density_e2: Option<i64> = row.get(2)?;
                let temp_k: Option<i64> = row.get(3)?;
                Ok(serde_json::json!({
                    "time_tag": time_tag,
                    "proton_speed":       speed_e1.map(|v| v as f64 / 10.0),
                    "proton_density":     density_e2.map(|v| v as f64 / 100.0),
                    "proton_temperature": temp_k.map(|v| v as f64),
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(serde_json::Value::Array(rows))
    }

    pub fn get_xray_recent(&self) -> Result<serde_json::Value, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT time_tag, energy, satellite, flux_e12, observed_flux_e12 FROM xray \
             ORDER BY time_tag ASC LIMIT 2880",
        )?;
        let rows = stmt
            .query_map([], |row| {
                let time_tag: String = row.get(0)?;
                let energy: String = row.get(1)?;
                let satellite: i32 = row.get(2)?;
                let flux_e12: i64 = row.get(3)?;
                let observed_flux_e12: i64 = row.get(4)?;
                Ok(serde_json::json!({
                    "time_tag":      time_tag,
                    "energy":        energy,
                    "satellite":     satellite,
                    "flux":          flux_e12 as f64 / 1e12,
                    "observed_flux": observed_flux_e12 as f64 / 1e12,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(serde_json::Value::Array(rows))
    }

    pub fn get_alerts_recent(&self) -> Result<serde_json::Value, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT product_id, issue_datetime, message FROM space_weather_alert \
             ORDER BY issue_datetime DESC LIMIT 50",
        )?;
        let rows = stmt
            .query_map([], |row| {
                let product_id: String = row.get(0)?;
                let issue_datetime: String = row.get(1)?;
                let message: String = row.get(2)?;
                Ok(serde_json::json!({
                    "product_id":     product_id,
                    "issue_datetime": issue_datetime,
                    "message":        message,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(serde_json::Value::Array(rows))
    }

    pub fn get_imf_recent(&self) -> Result<serde_json::Value, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT time_tag, bz_e2, bt_e2 FROM imf ORDER BY time_tag DESC LIMIT 1440")?;
        let rows = stmt
            .query_map([], |row| {
                let time_tag: String = row.get(0)?;
                let bz_e2: Option<i64> = row.get(1)?;
                let bt_e2: Option<i64> = row.get(2)?;
                Ok(serde_json::json!({
                    "time_tag": time_tag,
                    "bz_gsm":  bz_e2.map(|v| v as f64 / 100.0),
                    "bt":      bt_e2.map(|v| v as f64 / 100.0),
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(serde_json::Value::Array(rows))
    }

    pub fn get_dst_recent(&self) -> Result<serde_json::Value, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT time_tag, dst_nt FROM dst ORDER BY time_tag DESC LIMIT 1440")?;
        let rows = stmt
            .query_map([], |row| {
                let time_tag: String = row.get(0)?;
                let dst_nt: Option<i32> = row.get(1)?;
                Ok(serde_json::json!({
                    "time_tag": time_tag,
                    "dst_nt":   dst_nt,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(serde_json::Value::Array(rows))
    }

    pub fn get_iss_latest(&self) -> Result<serde_json::Value, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT ts, lat_e6, lon_e6, altitude_m, velocity_m_h FROM iss_position \
             ORDER BY ts DESC LIMIT 1",
        )?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            let ts: i64 = row.get(0)?;
            let lat_e6: i64 = row.get(1)?;
            let lon_e6: i64 = row.get(2)?;
            let altitude_m: i64 = row.get(3)?;
            let velocity_m_h: i64 = row.get(4)?;
            Ok(serde_json::json!({
                "timestamp": ts,
                "latitude":  lat_e6 as f64 / 1_000_000.0,
                "longitude": lon_e6 as f64 / 1_000_000.0,
                "altitude":  altitude_m as f64 / 1_000.0,
                "velocity":  velocity_m_h as f64 / 1_000.0,
            }))
        } else {
            Ok(serde_json::Value::Null)
        }
    }

    pub fn get_apod_latest(&self) -> Result<serde_json::Value, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT date, title, explanation, url, media_type, hdurl FROM apod \
             ORDER BY date DESC LIMIT 1",
        )?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            let date: String = row.get(0)?;
            let title: String = row.get(1)?;
            let explanation: String = row.get(2)?;
            let url: String = row.get(3)?;
            let media_type: String = row.get(4)?;
            let hdurl: Option<String> = row.get(5)?;
            Ok(serde_json::json!({
                "date":        date,
                "title":       title,
                "explanation": explanation,
                "url":         url,
                "media_type":  media_type,
                "hdurl":       hdurl,
            }))
        } else {
            Ok(serde_json::Value::Null)
        }
    }

    pub fn get_epic_latest(&self) -> Result<serde_json::Value, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT identifier, caption, image, date, centroid_lat_e6, centroid_lon_e6 FROM epic \
             WHERE date = (SELECT MAX(date) FROM epic)",
        )?;
        let rows = stmt
            .query_map([], |row| {
                let identifier: String = row.get(0)?;
                let caption: String = row.get(1)?;
                let image: String = row.get(2)?;
                let date: String = row.get(3)?;
                let lat_e6: i64 = row.get(4)?;
                let lon_e6: i64 = row.get(5)?;
                Ok(serde_json::json!({
                    "identifier": identifier,
                    "caption":    caption,
                    "image":      image,
                    "date":       date,
                    "centroid_coordinates": {
                        "lat": lat_e6 as f64 / 1_000_000.0,
                        "lon": lon_e6 as f64 / 1_000_000.0,
                    },
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(serde_json::Value::Array(rows))
    }

    pub fn get_neo_recent(&self) -> Result<serde_json::Value, DbError> {
        let cutoff = now() - 7 * 24 * 3600;
        let mut stmt = self.conn.prepare(
            "SELECT id, close_approach_date, name, is_hazardous, \
                    diameter_min_m, diameter_max_m, velocity_m_per_h, miss_distance_m \
             FROM neo WHERE fetched_at > ? \
             ORDER BY close_approach_date ASC, id ASC",
        )?;
        let tuples = stmt
            .query_map([cutoff], |row| {
                let id: String = row.get(0)?;
                let date: String = row.get(1)?;
                let name: String = row.get(2)?;
                let is_hazardous: bool = row.get(3)?;
                let dmin_m: i64 = row.get(4)?;
                let dmax_m: i64 = row.get(5)?;
                let vel_scaled: i64 = row.get(6)?;
                let dist_scaled: i64 = row.get(7)?;
                Ok((
                    id,
                    date,
                    name,
                    is_hazardous,
                    dmin_m,
                    dmax_m,
                    vel_scaled,
                    dist_scaled,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut by_date: std::collections::HashMap<String, Vec<serde_json::Value>> =
            std::collections::HashMap::new();
        for (id, date, name, is_hazardous, dmin_m, dmax_m, vel_scaled, dist_scaled) in tuples {
            let dmin_km = dmin_m as f64 / 1_000.0;
            let dmax_km = dmax_m as f64 / 1_000.0;
            let vel_kmh = vel_scaled as f64 / 1_000.0;
            let dist_km = dist_scaled as f64 / 1_000.0;
            let obj = serde_json::json!({
                "id":   id,
                "name": name,
                "is_potentially_hazardous_asteroid": is_hazardous,
                "estimated_diameter": {
                    "kilometers": {
                        "estimated_diameter_min": dmin_km,
                        "estimated_diameter_max": dmax_km,
                    }
                },
                "close_approach_data": [{
                    "close_approach_date": date,
                    "relative_velocity": { "kilometers_per_hour": format!("{vel_kmh:.3}") },
                    "miss_distance":     { "kilometers": format!("{dist_km:.3}") },
                }],
            });
            by_date.entry(date.clone()).or_default().push(obj);
        }

        let element_count: usize = by_date.values().map(|v| v.len()).sum();
        Ok(serde_json::json!({
            "element_count":      element_count,
            "near_earth_objects": by_date,
        }))
    }

    pub fn get_exoplanets_all(&self) -> Result<serde_json::Value, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT pl_name, hostname, orbital_period_md, radius_me3, mass_me3, disc_year \
             FROM exoplanet ORDER BY disc_year DESC LIMIT 100",
        )?;
        let rows = stmt
            .query_map([], |row| {
                let pl_name: String = row.get(0)?;
                let hostname: String = row.get(1)?;
                let orbital_period_md: Option<i64> = row.get(2)?;
                let radius_me3: Option<i64> = row.get(3)?;
                let mass_me3: Option<i64> = row.get(4)?;
                let disc_year: Option<i32> = row.get(5)?;
                Ok(serde_json::json!({
                    "pl_name":  pl_name,
                    "hostname": hostname,
                    "pl_orbper": orbital_period_md.map(|v| v as f64 / 1_000.0),
                    "pl_rade":   radius_me3.map(|v| v as f64 / 1_000.0),
                    "pl_masse":  mass_me3.map(|v| v as f64 / 1_000.0),
                    "disc_year": disc_year,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(serde_json::Value::Array(rows))
    }
}

// ── Auth ──────────────────────────────────────────────────────────────────────

pub struct User {
    pub email: String,
    pub password_hash: String,
}

impl Db {
    pub fn create_user(&self, email: &str, hash: &str) -> Result<(), DbError> {
        let result = self.conn.execute(
            "INSERT INTO users (email, password_hash, created_at, plan) VALUES (?, ?, ?, 'starter')",
            params![email, hash, now()],
        );
        match result {
            Ok(_) => Ok(()),
            Err(e) if e.to_string().contains("Constraint Error") => Err(DbError::EmailTaken),
            Err(e) => Err(DbError::Duckdb(e)),
        }
    }

    pub fn update_password_hash(&self, email: &str, new_hash: &str) -> Result<(), DbError> {
        self.conn.execute(
            "UPDATE users SET password_hash = ? WHERE email = ?",
            params![new_hash, email],
        )?;
        Ok(())
    }

    pub fn find_user_by_email(&self, email: &str) -> Result<Option<User>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT email, password_hash FROM users WHERE email = ?")?;
        let mut rows = stmt.query([email])?;
        if let Some(row) = rows.next()? {
            Ok(Some(User {
                email: row.get(0)?,
                password_hash: row.get(1)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn get_user_me(&self, email: &str) -> Result<serde_json::Value, DbError> {
        let mut stmt = self.conn.prepare("SELECT plan FROM users WHERE email = ?")?;
        let mut rows = stmt.query([email])?;
        let plan = match rows.next()? {
            Some(row) => row.get::<_, Option<String>>(0)?.unwrap_or_else(|| "starter".to_string()),
            None => "starter".to_string(),
        };
        Ok(serde_json::json!({ "email": email, "plan": plan }))
    }
}

// ── ISS inserts ───────────────────────────────────────────────────────────────

impl Db {
    pub fn insert_iss_position(&self, p: &IssPosition) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO iss_position (ts, lat_e6, lon_e6, altitude_m, velocity_m_h)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT (ts) DO NOTHING",
            params![
                p.timestamp,
                scale(p.latitude, 1_000_000.0),
                scale(p.longitude, 1_000_000.0),
                scale(p.altitude, 1_000.0),
                scale(p.velocity, 1_000.0),
            ],
        )?;
        Ok(())
    }
}

// ── Kp forecast ───────────────────────────────────────────────────────────────

impl Db {
    pub fn insert_kp_forecast(&self, ts: i64, kp_e2: i64) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO kp_forecast (ts, kp_e2, fetched_at) VALUES (?, ?, ?)
             ON CONFLICT (ts) DO NOTHING",
            params![ts, kp_e2, now()],
        )?;
        Ok(())
    }

    /// Returns the (ts, kp_e2) with the highest predicted Kp among forecasts stored since `since`.
    pub fn get_kp_forecast_max_recent(&self, since: i64) -> Result<Option<(i64, i64)>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT ts, kp_e2 FROM kp_forecast WHERE fetched_at > ? ORDER BY kp_e2 DESC LIMIT 1",
        )?;
        let mut rows = stmt.query([since])?;
        if let Some(row) = rows.next()? {
            Ok(Some((row.get(0)?, row.get(1)?)))
        } else {
            Ok(None)
        }
    }
}

// ── Anomaly detection ─────────────────────────────────────────────────────────

impl Db {
    pub fn insert_anomaly(
        &self,
        anomaly_type: &str,
        source_ref: &str,
        severity: &str,
        message: &str,
    ) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO alerts_anomaly (anomaly_type, source_ref, detected_at, severity, message)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT (anomaly_type, source_ref) DO NOTHING",
            params![anomaly_type, source_ref, now(), severity, message],
        )?;
        Ok(())
    }

    pub fn get_anomalies_recent(&self) -> Result<serde_json::Value, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT anomaly_type, source_ref, detected_at, severity, message
             FROM alerts_anomaly ORDER BY detected_at DESC LIMIT 100",
        )?;
        let rows = stmt
            .query_map([], |row| {
                let anomaly_type: String = row.get(0)?;
                let source_ref: String = row.get(1)?;
                let detected_at: i64 = row.get(2)?;
                let severity: String = row.get(3)?;
                let message: String = row.get(4)?;
                Ok(serde_json::json!({
                    "type":        anomaly_type,
                    "source_ref":  source_ref,
                    "detected_at": detected_at,
                    "severity":    severity,
                    "message":     message,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(serde_json::Value::Array(rows))
    }

    // ── Raw queries for anomaly detection ─────────────────────────────────────

    pub fn latest_kp_raw(&self) -> Result<Option<(String, i64)>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT time_tag, estimated_kp_e2 FROM kp ORDER BY time_tag DESC LIMIT 1")?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            Ok(Some((row.get(0)?, row.get(1)?)))
        } else {
            Ok(None)
        }
    }

    pub fn latest_solar_wind_speed_raw(&self) -> Result<Option<(String, i64)>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT time_tag, speed_e1 FROM solar_wind \
             WHERE speed_e1 IS NOT NULL ORDER BY time_tag DESC LIMIT 1",
        )?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            Ok(Some((row.get(0)?, row.get(1)?)))
        } else {
            Ok(None)
        }
    }

    /// Returns (time_tag, flux_e12) for the 0.1-0.8 nm long band (M/X class classification).
    pub fn latest_xray_flux_raw(&self) -> Result<Option<(String, i64)>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT time_tag, flux_e12 FROM xray \
             WHERE energy = '0.1-0.8nm' ORDER BY time_tag DESC LIMIT 1",
        )?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            Ok(Some((row.get(0)?, row.get(1)?)))
        } else {
            Ok(None)
        }
    }

    /// Returns (id, close_approach_date, miss_distance_m) for NEO approaches under `max_dist_scaled`
    /// fetched within the last `since` seconds window.
    pub fn neo_close_approaches_raw(
        &self,
        max_dist_scaled: i64,
        since: i64,
    ) -> Result<Vec<(String, String, i64)>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, close_approach_date, miss_distance_m FROM neo \
             WHERE miss_distance_m < ? AND fetched_at > ? \
             ORDER BY miss_distance_m ASC",
        )?;
        let rows = stmt
            .query_map([max_dist_scaled, since], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

// ── Reports ───────────────────────────────────────────────────────────────────

fn flux_to_xray_class(flux_e12: i64) -> String {
    let f = flux_e12 as f64 / 1e12;
    if f >= 1e-4 {
        format!("X{:.1}", f / 1e-4)
    } else if f >= 1e-5 {
        format!("M{:.1}", f / 1e-5)
    } else if f >= 1e-6 {
        format!("C{:.1}", f / 1e-6)
    } else if f >= 1e-7 {
        format!("B{:.1}", f / 1e-7)
    } else {
        format!("A{:.1}", f / 1e-8)
    }
}

impl Db {
    pub fn get_report_summary(&self, since_secs: i64) -> Result<serde_json::Value, DbError> {
        let cutoff = now() - since_secs;

        // Kp: avg and max over the window
        let (kp_avg, kp_max, kp_count) = {
            let mut stmt = self.conn.prepare(
                "SELECT AVG(estimated_kp_e2), MAX(estimated_kp_e2), COUNT(*) \
                 FROM kp WHERE fetched_at > ?",
            )?;
            let mut rows = stmt.query([cutoff])?;
            match rows.next()? {
                Some(row) => {
                    let avg: Option<f64> = row.get(0)?;
                    let max: Option<i64> = row.get(1)?;
                    let cnt: i64 = row.get(2)?;
                    (avg, max, cnt)
                }
                None => (None, None, 0i64),
            }
        };

        // Max solar wind speed in km/s (speed_e1 / 10)
        let sw_max: Option<i64> = {
            let mut stmt = self.conn.prepare(
                "SELECT MAX(speed_e1) FROM solar_wind \
                 WHERE fetched_at > ? AND speed_e1 IS NOT NULL",
            )?;
            let mut rows = stmt.query([cutoff])?;
            match rows.next()? {
                Some(row) => row.get(0)?,
                None => None,
            }
        };

        // Max X-ray flux in 0.1-0.8 nm band
        let xray_max: Option<i64> = {
            let mut stmt = self.conn.prepare(
                "SELECT MAX(flux_e12) FROM xray \
                 WHERE energy = '0.1-0.8nm' AND fetched_at > ?",
            )?;
            let mut rows = stmt.query([cutoff])?;
            match rows.next()? {
                Some(row) => row.get(0)?,
                None => None,
            }
        };

        // Anomaly count in window
        let anomaly_count: i64 = {
            let mut stmt = self
                .conn
                .prepare("SELECT COUNT(*) FROM alerts_anomaly WHERE detected_at > ?")?;
            let mut rows = stmt.query([cutoff])?;
            match rows.next()? {
                Some(row) => row.get(0)?,
                None => 0,
            }
        };

        // Asteroid close approaches in the date window (today → today+N days)
        let asteroid_count: i64 = {
            use chrono::Duration as D;
            let today = chrono::Utc::now().date_naive();
            let end_date = today + D::days((since_secs / 86400).max(1));
            let today_s = today.format("%Y-%m-%d").to_string();
            let end_s = end_date.format("%Y-%m-%d").to_string();
            let mut stmt = self.conn.prepare(
                "SELECT COUNT(*) FROM neo \
                 WHERE close_approach_date >= ? AND close_approach_date <= ?",
            )?;
            let mut rows = stmt.query(params![today_s, end_s])?;
            match rows.next()? {
                Some(row) => row.get(0)?,
                None => 0,
            }
        };

        Ok(serde_json::json!({
            "range_secs":          since_secs,
            "kp_avg":              kp_avg.map(|v| (v / 100.0 * 100.0).round() / 100.0),
            "kp_max":              kp_max.map(|v| v as f64 / 100.0),
            "kp_count":            kp_count,
            "solar_wind_max_kms":  sw_max.map(|v| v as f64 / 10.0),
            "xray_max_flux":       xray_max.map(|v| v as f64 / 1e12),
            "xray_max_class":      xray_max.map(flux_to_xray_class)
                                           .unwrap_or_else(|| "—".to_owned()),
            "anomaly_count":       anomaly_count,
            "asteroid_approaches": asteroid_count,
        }))
    }

    pub fn get_report_csv(&self, since_secs: i64) -> Result<String, DbError> {
        let cutoff = now() - since_secs;
        let mut out = String::new();

        // Kp section
        out.push_str("time_tag,kp_index,estimated_kp\n");
        {
            let mut stmt = self.conn.prepare(
                "SELECT time_tag, kp_index, estimated_kp_e2 FROM kp \
                 WHERE fetched_at > ? ORDER BY time_tag ASC",
            )?;
            let rows: Vec<(String, i32, i64)> = stmt
                .query_map([cutoff], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
                .collect::<Result<_, _>>()?;
            for (tt, kp_i, kp_e2) in rows {
                out.push_str(&format!("{},{},{:.2}\n", tt, kp_i, kp_e2 as f64 / 100.0));
            }
        }

        out.push('\n');

        // Solar wind section
        out.push_str("time_tag,speed_kms,density_pcm3,temperature_k\n");
        {
            let mut stmt = self.conn.prepare(
                "SELECT time_tag, speed_e1, density_e2, temp_k FROM solar_wind \
                 WHERE fetched_at > ? ORDER BY time_tag ASC",
            )?;
            type WindRow = (String, Option<i64>, Option<i64>, Option<i64>);
            let rows: Vec<WindRow> = stmt
                .query_map([cutoff], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                })?
                .collect::<Result<_, _>>()?;
            for (tt, speed, density, temp) in rows {
                let spd = speed
                    .map(|v| format!("{:.1}", v as f64 / 10.0))
                    .unwrap_or_default();
                let den = density
                    .map(|v| format!("{:.2}", v as f64 / 100.0))
                    .unwrap_or_default();
                let tmp = temp.map(|v| v.to_string()).unwrap_or_default();
                out.push_str(&format!("{},{},{},{}\n", tt, spd, den, tmp));
            }
        }

        out.push('\n');

        // X-ray section (0.1-0.8 nm band only)
        out.push_str("time_tag,flux_wm2,xray_class\n");
        {
            let mut stmt = self.conn.prepare(
                "SELECT time_tag, flux_e12 FROM xray \
                 WHERE energy = '0.1-0.8nm' AND fetched_at > ? ORDER BY time_tag ASC",
            )?;
            let rows: Vec<(String, i64)> = stmt
                .query_map([cutoff], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<Result<_, _>>()?;
            for (tt, flux_e12) in rows {
                let class = flux_to_xray_class(flux_e12);
                out.push_str(&format!(
                    "{},{:.3e},{}\n",
                    tt,
                    flux_e12 as f64 / 1e12,
                    class
                ));
            }
        }

        Ok(out)
    }
}

// ── Public endpoints (no auth) ────────────────────────────────────────────────

impl Db {
    /// Returns the last 60 Kp readings oldest-first — same shape as /api/kp.
    pub fn get_kp_array_public(&self) -> Result<serde_json::Value, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT time_tag, kp_index, estimated_kp_e2 FROM kp ORDER BY time_tag DESC LIMIT 60",
        )?;
        let mut rows: Vec<serde_json::Value> = stmt
            .query_map([], |row| {
                let time_tag: String = row.get(0)?;
                let kp_index: i32 = row.get(1)?;
                let kp_e2: i64 = row.get(2)?;
                Ok(serde_json::json!({
                    "time_tag":     time_tag,
                    "kp_index":     kp_index,
                    "estimated_kp": kp_e2 as f64 / 100.0,
                }))
            })?
            .collect::<Result<_, _>>()?;
        rows.reverse(); // oldest-first for the chart
        Ok(serde_json::Value::Array(rows))
    }

    pub fn get_solar_wind_latest_public(&self) -> Result<serde_json::Value, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT speed_e1, density_e2, time_tag FROM solar_wind \
             WHERE speed_e1 IS NOT NULL ORDER BY time_tag DESC LIMIT 1",
        )?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            let speed_e1: Option<i64> = row.get(0)?;
            let density_e2: Option<i64> = row.get(1)?;
            let time_tag: String = row.get(2)?;
            Ok(serde_json::json!({
                "speed":    speed_e1.map(|v| v as f64 / 10.0),
                "density":  density_e2.map(|v| v as f64 / 100.0),
                "time_tag": time_tag,
            }))
        } else {
            Ok(serde_json::json!({ "speed": null, "density": null, "time_tag": null }))
        }
    }
}

// ── API keys ──────────────────────────────────────────────────────────────────

pub struct ApiKey {
    pub id: String,
    pub name: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
    pub request_count: i64,
}

impl Db {
    pub fn create_api_key(
        &self,
        id: &str,
        user_email: &str,
        key_hash: &str,
        name: &str,
    ) -> Result<(), DbError> {
        let result = self.conn.execute(
            "INSERT INTO api_keys (id, user_email, key_hash, name, created_at, request_count)
             VALUES (?, ?, ?, ?, ?, 0)",
            params![id, user_email, key_hash, name, now()],
        );
        match result {
            Ok(_) => Ok(()),
            Err(e) if e.to_string().contains("Constraint Error") => Err(DbError::KeyNotFound),
            Err(e) => Err(DbError::Duckdb(e)),
        }
    }

    pub fn list_api_keys(&self, user_email: &str) -> Result<Vec<ApiKey>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, created_at, last_used_at, request_count \
             FROM api_keys WHERE user_email = ? ORDER BY created_at DESC",
        )?;
        let rows = stmt
            .query_map([user_email], |row| {
                Ok(ApiKey {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    created_at: row.get(2)?,
                    last_used_at: row.get(3)?,
                    request_count: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Returns true if deleted, false if key not found for this user.
    pub fn delete_api_key(&self, id: &str, user_email: &str) -> Result<bool, DbError> {
        let n = self.conn.execute(
            "DELETE FROM api_keys WHERE id = ? AND user_email = ?",
            params![id, user_email],
        )?;
        Ok(n > 0)
    }

    /// Returns the user_email for the given key hash, if it exists.
    pub fn find_api_key_by_hash(&self, key_hash: &str) -> Result<Option<String>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT user_email FROM api_keys WHERE key_hash = ? LIMIT 1")?;
        let mut rows = stmt.query([key_hash])?;
        match rows.next()? {
            Some(row) => Ok(Some(row.get(0)?)),
            None => Ok(None),
        }
    }

    /// Increments request_count and sets last_used_at for the given key hash.
    pub fn touch_api_key(&self, key_hash: &str) -> Result<(), DbError> {
        self.conn.execute(
            "UPDATE api_keys SET last_used_at = ?, request_count = request_count + 1 WHERE key_hash = ?",
            params![now(), key_hash],
        )?;
        Ok(())
    }
}

// ── Starlink ──────────────────────────────────────────────────────────────────

impl Db {
    pub fn insert_starlink_batch(&self, sats: &[StarlinkSat]) -> Result<(), DbError> {
        if sats.is_empty() {
            return Ok(());
        }
        self.begin()?;
        let result = (|| {
            // Full replace: TLEs are always a fresh snapshot, so DELETE + INSERT
            // is faster than per-row upsert conflict checking on 10k+ rows.
            self.conn.execute_batch("DELETE FROM starlink")?;
            let mut stmt = self.conn.prepare(
                "INSERT INTO starlink (norad_id, name, tle_line1, tle_line2, fetched_at)
                 VALUES (?, ?, ?, ?, ?)",
            )?;
            for sat in sats {
                stmt.execute(params![
                    sat.norad_id,
                    sat.name,
                    sat.tle_line1,
                    sat.tle_line2,
                    now()
                ])?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => self.commit(),
            Err(e) => {
                self.rollback();
                Err(e)
            }
        }
    }

    pub fn get_starlink_all(&self) -> Result<serde_json::Value, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT norad_id, name, tle_line1, tle_line2 FROM starlink ORDER BY norad_id ASC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                let norad_id: i32 = row.get(0)?;
                let name: String = row.get(1)?;
                let line1: String = row.get(2)?;
                let line2: String = row.get(3)?;
                Ok(serde_json::json!({
                    "norad_id":  norad_id,
                    "name":      name,
                    "tle_line1": line1,
                    "tle_line2": line2,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(serde_json::Value::Array(rows))
    }
}
