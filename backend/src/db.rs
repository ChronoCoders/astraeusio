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
