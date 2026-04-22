#![allow(dead_code)]

use duckdb::{params, Connection};
use thiserror::Error;

use crate::{
    iss::IssPosition,
    nasa::{Apod, CloseApproach, EpicImage, Exoplanet, NearEarthObject},
    noaa::{KpRecord, SolarWindRecord, SpaceWeatherAlert, XRayRecord},
};

#[derive(Error, Debug)]
pub enum DbError {
    #[error("database error: {0}")]
    Duckdb(#[from] duckdb::Error),
    #[error("parse error for field '{field}': {value}")]
    Parse { field: &'static str, value: String },
    #[error("email already registered")]
    EmailTaken,
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
";

// ── Db ────────────────────────────────────────────────────────────────────────

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open(path: &str) -> Result<Self, DbError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
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
            params![a.date, a.title, a.explanation, a.url, a.media_type, a.hdurl, now()],
        )?;
        Ok(())
    }

    /// One row per (neo, close_approach) pair.
    pub fn insert_neo(
        &self,
        neo: &NearEarthObject,
        approach: &CloseApproach,
        fetched_at: i64,
    ) -> Result<(), DbError> {
        let vel = parse_f64("velocity_kmh", &approach.relative_velocity.kilometers_per_hour)?;
        let dist = parse_f64("miss_distance_km", &approach.miss_distance.kilometers)?;

        self.conn.execute(
            "INSERT INTO neo
             (id, close_approach_date, name, is_hazardous,
              diameter_min_m, diameter_max_m, velocity_m_per_h, miss_distance_m, fetched_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (id, close_approach_date) DO NOTHING",
            params![
                neo.id,
                approach.close_approach_date,
                neo.name,
                neo.is_potentially_hazardous_asteroid,
                scale(neo.estimated_diameter.kilometers.estimated_diameter_min, 1_000.0),
                scale(neo.estimated_diameter.kilometers.estimated_diameter_max, 1_000.0),
                scale(vel, 1_000.0),
                scale(dist, 1_000.0),
                fetched_at,
            ],
        )?;
        Ok(())
    }

    pub fn insert_epic_image(&self, img: &EpicImage) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO epic
             (identifier, caption, image, date, centroid_lat_e6, centroid_lon_e6, fetched_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (identifier) DO NOTHING",
            params![
                img.identifier,
                img.caption,
                img.image,
                img.date,
                scale(img.centroid_coordinates.lat, 1_000_000.0),
                scale(img.centroid_coordinates.lon, 1_000_000.0),
                now(),
            ],
        )?;
        Ok(())
    }

    pub fn insert_exoplanet(&self, exo: &Exoplanet) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO exoplanet
             (pl_name, hostname, orbital_period_md, radius_me3, mass_me3, disc_year, fetched_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (pl_name) DO NOTHING",
            params![
                exo.pl_name,
                exo.hostname,
                scale_opt(exo.pl_orbper, 1_000.0),
                scale_opt(exo.pl_rade, 1_000.0),
                scale_opt(exo.pl_masse, 1_000.0),
                exo.disc_year,
                now(),
            ],
        )?;
        Ok(())
    }
}

// ── NOAA inserts ──────────────────────────────────────────────────────────────

impl Db {
    pub fn insert_kp(&self, r: &KpRecord) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO kp (time_tag, kp_index, estimated_kp_e2, fetched_at)
             VALUES (?, ?, ?, ?)
             ON CONFLICT (time_tag) DO NOTHING",
            params![r.time_tag, r.kp_index, scale(r.estimated_kp, 100.0), now()],
        )?;
        Ok(())
    }

    pub fn insert_solar_wind(&self, r: &SolarWindRecord) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO solar_wind (time_tag, speed_e1, density_e2, temp_k, fetched_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT (time_tag) DO NOTHING",
            params![
                r.time_tag,
                scale_opt(r.proton_speed, 10.0),
                scale_opt(r.proton_density, 100.0),
                r.proton_temperature.map(|t| t.round() as i64),
                now(),
            ],
        )?;
        Ok(())
    }

    pub fn insert_xray(&self, r: &XRayRecord) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO xray
             (time_tag, energy, satellite, flux_e12, observed_flux_e12, fetched_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT (time_tag, energy) DO NOTHING",
            params![
                r.time_tag,
                r.energy,
                r.satellite,
                scale(r.flux, 1e12),
                scale(r.observed_flux, 1e12),
                now(),
            ],
        )?;
        Ok(())
    }

    pub fn insert_alert(&self, a: &SpaceWeatherAlert) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO space_weather_alert (product_id, issue_datetime, message, fetched_at)
             VALUES (?, ?, ?, ?)
             ON CONFLICT (product_id, issue_datetime) DO NOTHING",
            params![a.product_id, a.issue_datetime, a.message, now()],
        )?;
        Ok(())
    }
}

// ── NOAA queries ─────────────────────────────────────────────────────────────

impl Db {
    /// Returns the `n` most recent Kp readings ordered oldest-first.
    pub fn get_recent_kp(&self, n: usize) -> Result<Vec<f64>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT estimated_kp_e2 FROM kp ORDER BY time_tag DESC LIMIT ?",
        )?;
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
                    "kp": "",
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
                Ok((id, date, name, is_hazardous, dmin_m, dmax_m, vel_scaled, dist_scaled))
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
            "INSERT INTO users (email, password_hash, created_at) VALUES (?, ?, ?)",
            params![email, hash, now()],
        );
        match result {
            Ok(_) => Ok(()),
            Err(e) if e.to_string().contains("Constraint Error") => Err(DbError::EmailTaken),
            Err(e) => Err(DbError::Duckdb(e)),
        }
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
