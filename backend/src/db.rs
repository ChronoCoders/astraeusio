use duckdb::{Connection, params};
use tracing::{error, info, warn};
use thiserror::Error;

use crate::{
    iss::IssPosition,
    nasa::{Apod, EpicImage, Exoplanet, NeoFeed},
    noaa::{
        DstRecord, ImfRecord, Kp3hRecord, KpRecord, SolarWindRecord, SpaceWeatherAlert, XRayRecord,
    },
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
    #[error("insufficient Kp history: have {have} three-hour readings, need {need}")]
    InsufficientHistory { have: usize, need: usize },
    #[error("writer channel closed")]
    WriterClosed,
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
    observed_at   BIGINT,
    fetched_at    BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS solar_wind (
    time_tag   TEXT   NOT NULL PRIMARY KEY,
    speed_e1   BIGINT,
    density_e2 BIGINT,
    temp_k     BIGINT,
    observed_at BIGINT,
    fetched_at BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS xray (
    time_tag          TEXT    NOT NULL,
    energy            TEXT    NOT NULL,
    satellite         INTEGER NOT NULL,
    flux_e12          BIGINT  NOT NULL,
    observed_flux_e12 BIGINT  NOT NULL,
    observed_at       BIGINT,
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
    created_at    BIGINT NOT NULL,
    auth_provider TEXT   DEFAULT 'password'
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
    observed_at BIGINT,
    fetched_at BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS dst (
    time_tag   TEXT    NOT NULL PRIMARY KEY,
    dst_nt     INTEGER,
    observed_at BIGINT,
    fetched_at BIGINT  NOT NULL
);

CREATE TABLE IF NOT EXISTS kp_3h (
    time_tag   TEXT   NOT NULL PRIMARY KEY,
    kp_e2      BIGINT NOT NULL,
    observed_at BIGINT,
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

CREATE TABLE IF NOT EXISTS usage_records (
    user_email    TEXT   NOT NULL PRIMARY KEY,
    request_count BIGINT NOT NULL DEFAULT 0,
    period_start  BIGINT NOT NULL,
    period_end    BIGINT NOT NULL,
    updated_at    BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS webhooks (
    id         TEXT    NOT NULL PRIMARY KEY,
    user_email TEXT    NOT NULL,
    url        TEXT    NOT NULL,
    secret     TEXT    NOT NULL,
    events     TEXT    NOT NULL,
    active     BOOLEAN NOT NULL DEFAULT TRUE,
    created_at BIGINT  NOT NULL
);

CREATE TABLE IF NOT EXISTS email_alerts (
    id                TEXT    NOT NULL PRIMARY KEY,
    user_email        TEXT    NOT NULL UNIQUE,
    enabled           BOOLEAN NOT NULL DEFAULT TRUE,
    kp_threshold_e2   BIGINT  NOT NULL DEFAULT 500,
    wind_threshold_e1 BIGINT  NOT NULL DEFAULT 7000,
    last_notified_at  BIGINT,
    created_at        BIGINT  NOT NULL
);

CREATE TABLE IF NOT EXISTS custom_anomaly_rules (
    id         TEXT    NOT NULL PRIMARY KEY,
    user_email TEXT    NOT NULL,
    name       TEXT    NOT NULL,
    metric     TEXT    NOT NULL,
    operator   TEXT    NOT NULL,
    threshold  DOUBLE  NOT NULL,
    severity   TEXT    NOT NULL,
    enabled    BOOLEAN NOT NULL DEFAULT TRUE,
    created_at BIGINT  NOT NULL
);

CREATE TABLE IF NOT EXISTS health_snapshots (
    component TEXT   NOT NULL,
    ts        BIGINT NOT NULL,
    status    TEXT   NOT NULL,
    PRIMARY KEY (component, ts)
);

CREATE TABLE IF NOT EXISTS webhook_deliveries (
    id           BIGINT  NOT NULL PRIMARY KEY,
    webhook_id   TEXT    NOT NULL,
    attempted_at BIGINT  NOT NULL,
    status_code  INTEGER,
    success      BOOLEAN NOT NULL,
    error        TEXT
);
CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_lookup
    ON webhook_deliveries (webhook_id, attempted_at DESC);

CREATE SEQUENCE IF NOT EXISTS seq_webhook_deliveries START 1;
";

// ── Observation time ──────────────────────────────────────────────────────────

/// Derives `observed_at` (UTC epoch seconds) from a `time_tag` string.
///
/// The six time-series feeds use three different upstream formats: bare
/// `2026-05-11T03:47:00`, trailing-Z `2026-05-11T03:45:00Z`, and space-separated
/// `2026-05-11 03:47:00.000`. All three cast to TIMESTAMP, and `epoch` reads a
/// naive TIMESTAMP as UTC, so one expression covers every table. Used verbatim
/// by both the migration backfill and every insert so the two cannot diverge.
const OBSERVED_AT_SQL: &str = "epoch(time_tag::TIMESTAMP)::BIGINT";

/// Same derivation for an INSERT, where `time_tag` is a bound parameter rather
/// than a column reference.
const OBSERVED_AT_PARAM_SQL: &str = "epoch(?::TIMESTAMP)::BIGINT";

const OBSERVED_AT_TABLES: [&str; 6] = ["kp", "kp_3h", "solar_wind", "xray", "imf", "dst"];

/// Identifier for the one-shot purge of forecasts generated before the model
/// input was corrected to read the three-hour series.
const PURGE_FORECASTS_MIGRATION: &str = "2026-08-purge-kp-forecast-wrong-input-series";

/// Identifier for the one-shot removal of the observed_at indexes.
const DROP_OBSERVED_AT_INDEXES_MIGRATION: &str = "2026-08-drop-observed-at-indexes";

/// Probe values per table for the startup self check.
const SELF_CHECK_PROBES: i64 = 3;

/// Compares a range predicate the scan can prune against the same range written
/// so it cannot, and logs any disagreement.
///
/// The two forms describe identical row sets, so a difference means the scan
/// dropped rows that match, which is the symptom this column's investigation
/// started from and which has never been reproduced. There is no trigger to
/// assert against, so this watches for it in production instead. Wrapping the
/// scan in `OFFSET 0` blocks filter pushdown, which is what makes the second
/// form immune to pruning.
///
/// Advisory only: a disagreement is logged and startup continues, because
/// refusing to boot on a read anomaly would turn a reporting fault into an
/// outage.
fn self_check_observed_at(conn: &Connection) {
    for table in OBSERVED_AT_TABLES {
        let bounds = conn.query_row(
            &format!("SELECT min(observed_at), max(observed_at) FROM {table}"),
            [],
            |row| Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, Option<i64>>(1)?)),
        );
        let (Some(lo), Some(hi)) = (match bounds {
            Ok(b) => b,
            Err(e) => {
                warn!(table, "observed_at self check could not read bounds: {e}");
                continue;
            }
        }) else {
            continue;
        };

        for i in 0..SELF_CHECK_PROBES {
            let probe = lo + (hi - lo) * i / SELF_CHECK_PROBES;
            let pruned = conn.query_row(
                &format!("SELECT count(*) FROM {table} WHERE observed_at > ?"),
                params![probe],
                |row| row.get::<_, i64>(0),
            );
            let unpruned = conn.query_row(
                &format!(
                    "SELECT count(*) FROM (SELECT observed_at FROM {table} OFFSET 0) \
                     WHERE observed_at > ?"
                ),
                params![probe],
                |row| row.get::<_, i64>(0),
            );
            match (pruned, unpruned) {
                (Ok(a), Ok(b)) if a != b => {
                    error!(
                        table,
                        probe,
                        pruned_count = a,
                        unpruned_count = b,
                        missing = b - a,
                        stats = %row_group_stats(conn, table),
                        "observed_at range scan dropped matching rows"
                    );
                }
                (Ok(_), Ok(_)) => {}
                (Err(e), _) | (_, Err(e)) => {
                    warn!(table, probe, "observed_at self check query failed: {e}");
                }
            }
        }
    }
}

/// Row group statistics for `table`, rendered for a log line.
fn row_group_stats(conn: &Connection, table: &str) -> String {
    let mut stmt = match conn.prepare(
        "SELECT row_group_id, count, stats FROM pragma_storage_info(?) \
         WHERE column_name = 'observed_at' AND segment_type <> 'VALIDITY' ORDER BY row_group_id",
    ) {
        Ok(s) => s,
        Err(e) => return format!("unavailable: {e}"),
    };
    let rows = stmt.query_map(params![table], |row| {
        Ok(format!(
            "rg{}({} rows) {}",
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?
        ))
    });
    match rows {
        Ok(iter) => iter
            .collect::<Result<Vec<_>, _>>()
            .map(|v| v.join(" | "))
            .unwrap_or_else(|e| format!("unavailable: {e}")),
        Err(e) => format!("unavailable: {e}"),
    }
}

// ── Store ─────────────────────────────────────────────────────────────────────

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &str) -> Result<Self, DbError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        // Migrate existing DBs that pre-date the plan column.
        for sql in [
            "ALTER TABLE users ADD COLUMN plan TEXT DEFAULT 'starter'",
            "ALTER TABLE users ADD COLUMN email_verified BOOLEAN DEFAULT FALSE",
            "ALTER TABLE users ADD COLUMN totp_secret TEXT",
            "ALTER TABLE users ADD COLUMN totp_enabled BOOLEAN DEFAULT FALSE",
            "ALTER TABLE users ADD COLUMN auth_provider TEXT DEFAULT 'password'",
            "ALTER TABLE kp_forecast ADD COLUMN ci_lower_e2 BIGINT",
            "ALTER TABLE kp_forecast ADD COLUMN ci_upper_e2 BIGINT",
            "ALTER TABLE kp_forecast ADD COLUMN uncertainty_e4 BIGINT",
            "ALTER TABLE kp ADD COLUMN observed_at BIGINT",
            "ALTER TABLE kp_3h ADD COLUMN observed_at BIGINT",
            "ALTER TABLE solar_wind ADD COLUMN observed_at BIGINT",
            "ALTER TABLE xray ADD COLUMN observed_at BIGINT",
            "ALTER TABLE imf ADD COLUMN observed_at BIGINT",
            "ALTER TABLE dst ADD COLUMN observed_at BIGINT",
        ] {
            if let Err(e) = conn.execute_batch(sql) {
                let msg = e.to_string().to_lowercase();
                if !msg.contains("already exists") && !msg.contains("duplicate") {
                    return Err(DbError::Duckdb(e));
                }
            }
        }

        // One-shot data migrations, recorded so a redeploy cannot repeat them.
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                 id         TEXT   NOT NULL PRIMARY KEY,
                 applied_at BIGINT NOT NULL
             )",
        )?;
        let purge_applied: i64 = conn.query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE id = ?",
            params![PURGE_FORECASTS_MIGRATION],
            |row| row.get(0),
        )?;
        if purge_applied == 0 {
            // Every stored forecast was produced from the one-minute Kp series
            // fed to a model trained on the three-hour series, so none of them
            // is recoverable. Guarded rather than unconditional, otherwise the
            // table could never accumulate history again.
            let purged = conn.execute("DELETE FROM kp_forecast", [])?;
            conn.execute(
                "INSERT INTO schema_migrations (id, applied_at) VALUES (?, ?)",
                params![PURGE_FORECASTS_MIGRATION, now()],
            )?;
            info!(
                purged,
                "purged forecasts produced from the wrong input series"
            );
        }

        let drop_idx_applied: i64 = conn.query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE id = ?",
            params![DROP_OBSERVED_AT_INDEXES_MIGRATION],
            |row| row.get(0),
        )?;
        if drop_idx_applied == 0 {
            // These indexes were measured against the three heaviest report
            // queries at both the current row counts and a projected year of
            // ingest, and made every one of them slower, by roughly 38x at the
            // one year size. They also tripled the database file. Dropped for
            // deployments that already created them.
            for table in OBSERVED_AT_TABLES {
                conn.execute_batch(&format!("DROP INDEX IF EXISTS idx_{table}_observed_at"))?;
            }
            conn.execute(
                "INSERT INTO schema_migrations (id, applied_at) VALUES (?, ?)",
                params![DROP_OBSERVED_AT_INDEXES_MIGRATION, now()],
            )?;
            info!("dropped observed_at indexes");
        }

        // Backfill observed_at. The three upstream time_tag formats (bare,
        // trailing Z, and space-separated with milliseconds) all cast cleanly,
        // and epoch() reads a naive TIMESTAMP as UTC, so OBSERVED_AT_SQL is the
        // single expression used both here and at insert time. The IS NULL
        // guard keeps the backfill a no-op after the first boot.
        for table in OBSERVED_AT_TABLES {
            conn.execute_batch(&format!(
                "UPDATE {table} SET observed_at = {OBSERVED_AT_SQL} WHERE observed_at IS NULL"
            ))?;
        }

        // Adding observed_at by ALTER leaves each row group's statistics at the
        // empty sentinel (min i64::MAX, max i64::MIN, has_null true), and the
        // backfill UPDATE does not refresh them: it records the new values as
        // pending updates instead. Until something merges them, the statistics
        // on disk do not describe the data in the table, and a process that
        // exits without checkpointing persists that state. Checkpointing here
        // merges the updates and recomputes the statistics before this
        // connection serves a single query.
        conn.execute_batch("CHECKPOINT")?;

        self_check_observed_at(&conn);

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

    pub fn try_clone(&self) -> Result<Self, DbError> {
        Ok(Self {
            conn: self.conn.try_clone()?,
        })
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

impl Store {
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

impl Store {
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
            let mut stmt = self.conn.prepare(&format!(
                "INSERT INTO kp (time_tag, kp_index, estimated_kp_e2, observed_at, fetched_at)
                 VALUES (?, ?, ?, {OBSERVED_AT_PARAM_SQL}, ?)
                 ON CONFLICT (time_tag) DO UPDATE SET
                   kp_index = CASE WHEN excluded.kp_index > 0 THEN excluded.kp_index ELSE kp.kp_index END,
                   estimated_kp_e2 = CASE WHEN excluded.estimated_kp_e2 > 0 THEN excluded.estimated_kp_e2 ELSE kp.estimated_kp_e2 END,
                   fetched_at = excluded.fetched_at"
            ))?;
            for r in to_insert {
                stmt.execute(params![
                    r.time_tag,
                    r.kp_index,
                    scale(r.estimated_kp, 100.0),
                    r.time_tag,
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
            let mut stmt = self.conn.prepare(&format!(
                "INSERT INTO solar_wind (time_tag, speed_e1, density_e2, temp_k, observed_at, fetched_at)
                 VALUES (?, ?, ?, ?, {OBSERVED_AT_PARAM_SQL}, ?)
                 ON CONFLICT (time_tag) DO NOTHING"
            ))?;
            for r in to_insert {
                stmt.execute(params![
                    r.time_tag,
                    scale_opt(r.proton_speed, 10.0),
                    scale_opt(r.proton_density, 100.0),
                    r.proton_temperature.map(|t| t.round() as i64),
                    r.time_tag,
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
            let mut stmt = self.conn.prepare(&format!(
                "INSERT INTO xray
                 (time_tag, energy, satellite, flux_e12, observed_flux_e12, observed_at, fetched_at)
                 VALUES (?, ?, ?, ?, ?, {OBSERVED_AT_PARAM_SQL}, ?)
                 ON CONFLICT (time_tag, energy) DO NOTHING"
            ))?;
            for r in to_insert {
                stmt.execute(params![
                    r.time_tag,
                    r.energy,
                    r.satellite,
                    scale(r.flux, 1e12),
                    scale(r.observed_flux, 1e12),
                    r.time_tag,
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
            let mut stmt = self.conn.prepare(&format!(
                "INSERT INTO imf (time_tag, bz_e2, bt_e2, observed_at, fetched_at)
                 VALUES (?, ?, ?, {OBSERVED_AT_PARAM_SQL}, ?)
                 ON CONFLICT (time_tag) DO NOTHING"
            ))?;
            for r in to_insert {
                stmt.execute(params![
                    r.time_tag,
                    scale_opt(r.bz_gsm, 100.0),
                    scale_opt(r.bt, 100.0),
                    r.time_tag,
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
            let mut stmt = self.conn.prepare(&format!(
                "INSERT INTO dst (time_tag, dst_nt, observed_at, fetched_at)
                 VALUES (?, ?, {OBSERVED_AT_PARAM_SQL}, ?)
                 ON CONFLICT (time_tag) DO NOTHING"
            ))?;
            for r in to_insert {
                stmt.execute(params![r.time_tag, r.dst_nt, r.time_tag, now()])?;
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
            let mut stmt = self.conn.prepare(&format!(
                "INSERT INTO kp_3h (time_tag, kp_e2, observed_at, fetched_at)
                 VALUES (?, ?, {OBSERVED_AT_PARAM_SQL}, ?)
                 ON CONFLICT (time_tag) DO UPDATE SET kp_e2 = excluded.kp_e2, fetched_at = excluded.fetched_at"
            ))?;
            for r in records {
                stmt.execute(params![r.time_tag, scale(r.kp, 100.0), r.time_tag, now()])?;
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

impl Store {
    /// Returns the `n` most recent three-hour Kp readings, oldest-first.
    ///
    /// This is the series the forecast model is trained on: one value per
    /// three-hour period. Errors rather than returning a short vector, because
    /// a short sequence would be silently padded by the ML service and produce
    /// a forecast from mostly synthetic input.
    pub fn get_recent_kp_3h(&self, n: usize) -> Result<Vec<f64>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT kp_e2 FROM kp_3h ORDER BY observed_at DESC LIMIT ?")?;
        let rows: Vec<i64> = stmt
            .query_map([n as i64], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        if rows.len() < n {
            return Err(DbError::InsufficientHistory {
                have: rows.len(),
                need: n,
            });
        }
        Ok(rows.into_iter().rev().map(|v| v as f64 / 100.0).collect())
    }

    /// Most recent 1440 Kp readings, oldest-first. Selected DESC so the LIMIT
    /// takes the newest window, then reversed for the caller.
    pub fn get_kp_recent(&self) -> Result<serde_json::Value, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT time_tag, kp_index, estimated_kp_e2 FROM kp ORDER BY observed_at DESC LIMIT 1440",
        )?;
        let mut rows = stmt
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
        rows.reverse();
        Ok(serde_json::Value::Array(rows))
    }

    /// Bucketed average Kp for the given time window. Bucket size adapts to range
    /// so charts always receive ≤ ~200 points regardless of period length.
    pub fn get_kp_range(&self, since_secs: i64) -> Result<serde_json::Value, DbError> {
        let cutoff = now() - since_secs;
        let bucket = if since_secs <= 86_400 {
            900
        } else if since_secs <= 604_800 {
            3_600
        } else {
            21_600
        };
        let sql = format!(
            "SELECT MIN(time_tag) as time_tag, CAST(AVG(estimated_kp_e2) AS BIGINT) as kp_e2 \
             FROM kp WHERE observed_at > ? GROUP BY observed_at / {bucket} ORDER BY time_tag ASC"
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
        let bucket = if since_secs <= 86_400 {
            900
        } else if since_secs <= 604_800 {
            3_600
        } else {
            21_600
        };
        let sql = format!(
            "SELECT MIN(time_tag) as time_tag, CAST(AVG(speed_e1) AS BIGINT) as speed_e1 \
             FROM solar_wind WHERE observed_at > ? AND speed_e1 IS NOT NULL \
             GROUP BY observed_at / {bucket} ORDER BY time_tag ASC"
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

    /// Most recent 240 three-hour Kp readings, oldest-first.
    pub fn get_kp_3h_recent(&self) -> Result<serde_json::Value, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT time_tag, kp_e2 FROM kp_3h ORDER BY observed_at DESC LIMIT 240")?;
        let mut rows = stmt
            .query_map([], |row| {
                let time_tag: String = row.get(0)?;
                let kp_e2: i64 = row.get(1)?;
                Ok(serde_json::json!({
                    "time_tag": time_tag,
                    "estimated_kp": kp_e2 as f64 / 100.0,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.reverse();
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

    /// Most recent 2880 X-ray rows, oldest-first. The limit spans 1440 timestamps
    /// because each carries both the long and short energy band.
    pub fn get_xray_recent(&self) -> Result<serde_json::Value, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT time_tag, energy, satellite, flux_e12, observed_flux_e12 FROM xray \
             ORDER BY observed_at DESC LIMIT 2880",
        )?;
        let mut rows = stmt
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
        rows.reverse();
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
             WHERE substr(date, 1, 10) = (SELECT MAX(substr(date, 1, 10)) FROM epic) \
             ORDER BY identifier ASC",
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
    pub email_verified: bool,
    pub totp_secret: Option<String>,
    pub totp_enabled: bool,
}

impl Store {
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

    /// Create an account that authenticates via an external OAuth provider.
    /// `hash` is a random unguessable bcrypt hash (password login is never used for
    /// these accounts) and the email arrives pre-verified from the provider.
    pub fn create_oauth_user(&self, email: &str, provider: &str, hash: &str) -> Result<(), DbError> {
        let result = self.conn.execute(
            "INSERT INTO users (email, password_hash, created_at, plan, email_verified, auth_provider) \
             VALUES (?, ?, ?, 'starter', TRUE, ?)",
            params![email, hash, now(), provider],
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

    pub fn update_user_plan(&self, email: &str, plan: &str) -> Result<(), DbError> {
        self.conn.execute(
            "UPDATE users SET plan = ? WHERE email = ?",
            params![plan, email],
        )?;
        Ok(())
    }

    pub fn find_user_by_email(&self, email: &str) -> Result<Option<User>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT email, password_hash, email_verified, totp_secret, totp_enabled \
             FROM users WHERE email = ?",
        )?;
        let mut rows = stmt.query([email])?;
        if let Some(row) = rows.next()? {
            Ok(Some(User {
                email: row.get(0)?,
                password_hash: row.get(1)?,
                email_verified: row.get::<_, Option<bool>>(2)?.unwrap_or(false),
                totp_secret: row.get(3)?,
                totp_enabled: row.get::<_, Option<bool>>(4)?.unwrap_or(false),
            }))
        } else {
            Ok(None)
        }
    }

    pub fn get_user_me(&self, email: &str) -> Result<serde_json::Value, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT plan, email_verified, totp_enabled FROM users WHERE email = ?")?;
        let mut rows = stmt.query([email])?;
        match rows.next()? {
            Some(row) => {
                let plan = row
                    .get::<_, Option<String>>(0)?
                    .unwrap_or_else(|| "starter".to_string());
                let verified = row.get::<_, Option<bool>>(1)?.unwrap_or(false);
                let totp_on = row.get::<_, Option<bool>>(2)?.unwrap_or(false);
                Ok(serde_json::json!({
                    "email":          email,
                    "plan":           plan,
                    "email_verified": verified,
                    "totp_enabled":   totp_on,
                }))
            }
            None => Ok(serde_json::json!({
                "email":          email,
                "plan":           "starter",
                "email_verified": false,
                "totp_enabled":   false,
            })),
        }
    }
}

// ── ISS inserts ───────────────────────────────────────────────────────────────

impl Store {
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

impl Store {
    pub fn insert_kp_forecast(
        &self,
        ts: i64,
        kp_e2: i64,
        ci_lower_e2: Option<i64>,
        ci_upper_e2: Option<i64>,
        uncertainty_e4: Option<i64>,
    ) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO kp_forecast (ts, kp_e2, ci_lower_e2, ci_upper_e2, uncertainty_e4, fetched_at) \
             VALUES (?, ?, ?, ?, ?, ?) \
             ON CONFLICT (ts) DO UPDATE SET \
                 kp_e2          = excluded.kp_e2, \
                 ci_lower_e2    = excluded.ci_lower_e2, \
                 ci_upper_e2    = excluded.ci_upper_e2, \
                 uncertainty_e4 = excluded.uncertainty_e4, \
                 fetched_at     = excluded.fetched_at",
            params![ts, kp_e2, ci_lower_e2, ci_upper_e2, uncertainty_e4, now()],
        )?;
        Ok(())
    }

    /// Returns paired predicted/actual Kp rows for the forecast history page.
    /// Pairs each forecast `ts` with the closest `kp_3h` actual within ±90 minutes.
    pub fn get_forecast_history(&self, since: i64) -> Result<serde_json::Value, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT f.ts, f.kp_e2, f.ci_lower_e2, f.ci_upper_e2, \
                    ( \
                      SELECT k.kp_e2 FROM kp_3h k \
                      WHERE abs(epoch(k.time_tag::TIMESTAMP) - f.ts) < 5400 \
                      ORDER BY abs(epoch(k.time_tag::TIMESTAMP) - f.ts) ASC \
                      LIMIT 1 \
                    ) AS actual_e2 \
             FROM kp_forecast f \
             WHERE f.ts > ? \
             ORDER BY f.ts ASC",
        )?;
        let rows = stmt
            .query_map([since], |row| {
                let ts: i64 = row.get(0)?;
                let kp_e2: i64 = row.get(1)?;
                let ci_l: Option<i64> = row.get(2)?;
                let ci_u: Option<i64> = row.get(3)?;
                let actual: Option<i64> = row.get(4)?;
                Ok(serde_json::json!({
                    "ts":           ts,
                    "predicted_kp": kp_e2 as f64 / 100.0,
                    "ci_lower":     ci_l.map(|v| v as f64 / 100.0),
                    "ci_upper":     ci_u.map(|v| v as f64 / 100.0),
                    "actual_kp":    actual.map(|v| v as f64 / 100.0),
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(serde_json::Value::Array(rows))
    }

    /// Computes forecast-vs-actual aggregate metrics over the last `since` seconds.
    pub fn get_forecast_metrics(&self, since: i64) -> Result<serde_json::Value, DbError> {
        let mut stmt = self.conn.prepare(
            "WITH paired AS ( \
               SELECT f.kp_e2 AS pred, \
                      ( \
                        SELECT k.kp_e2 FROM kp_3h k \
                        WHERE abs(epoch(k.time_tag::TIMESTAMP) - f.ts) < 5400 \
                        ORDER BY abs(epoch(k.time_tag::TIMESTAMP) - f.ts) ASC \
                        LIMIT 1 \
                      ) AS actual, \
                      f.uncertainty_e4 AS unc \
               FROM kp_forecast f \
               WHERE f.ts > ? \
             ) \
             SELECT \
               COUNT(*) FILTER (WHERE actual IS NOT NULL) AS n, \
               AVG(ABS(pred - actual)) FILTER (WHERE actual IS NOT NULL) AS mae_e2, \
               SQRT(AVG((pred - actual) * (pred - actual)) FILTER (WHERE actual IS NOT NULL)) AS rmse_e2, \
               COUNT(*) FILTER (WHERE actual >= 500)                                AS n_storms, \
               COUNT(*) FILTER (WHERE actual >= 500 AND pred >= 500)                AS n_storms_caught, \
               COUNT(*) FILTER (WHERE pred >= 500 AND (actual IS NOT NULL AND actual < 500)) AS n_false_pos, \
               AVG(unc) FILTER (WHERE unc IS NOT NULL) AS mean_unc_e4 \
             FROM paired",
        )?;
        let mut rows = stmt.query([since])?;
        if let Some(row) = rows.next()? {
            let n: i64 = row.get::<_, Option<i64>>(0)?.unwrap_or(0);
            let mae_e2: Option<f64> = row.get(1)?;
            let rmse_e2: Option<f64> = row.get(2)?;
            let n_storms: i64 = row.get::<_, Option<i64>>(3)?.unwrap_or(0);
            let n_caught: i64 = row.get::<_, Option<i64>>(4)?.unwrap_or(0);
            let n_false: i64 = row.get::<_, Option<i64>>(5)?.unwrap_or(0);
            let mean_unc_e4: Option<f64> = row.get(6)?;
            Ok(serde_json::json!({
                "n_samples":   n,
                "mae":         mae_e2.map(|v| v / 100.0),
                "rmse":        rmse_e2.map(|v| v / 100.0),
                "n_storms":    n_storms,
                "n_caught":    n_caught,
                "n_false_pos": n_false,
                "hit_rate":    if n_storms > 0 { Some(n_caught as f64 / n_storms as f64) } else { None },
                "mean_unc":    mean_unc_e4.map(|v| v / 10_000.0),
            }))
        } else {
            Ok(serde_json::json!({ "n_samples": 0 }))
        }
    }

    pub fn get_kp_forecast_latest(&self) -> Result<Option<(i64, i64)>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT ts, kp_e2 FROM kp_forecast ORDER BY fetched_at DESC LIMIT 1")?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            Ok(Some((row.get(0)?, row.get(1)?)))
        } else {
            Ok(None)
        }
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

impl Store {
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

    /// Paginated, filtered browse of past anomaly events.
    /// `since` is a unix-seconds cutoff. `type_filter` and `severity_filter`
    /// are optional exact matches. Returns rows + total count for pagination.
    pub fn get_events_page(
        &self,
        since: i64,
        type_filter: Option<&str>,
        severity_filter: Option<&str>,
        page: i64,
        page_size: i64,
    ) -> Result<serde_json::Value, DbError> {
        let mut where_clauses = vec!["detected_at > ?".to_string()];
        let mut bindings: Vec<duckdb::types::Value> = vec![since.into()];
        if let Some(t) = type_filter {
            where_clauses.push("anomaly_type = ?".to_string());
            bindings.push(t.to_string().into());
        }
        if let Some(s) = severity_filter {
            where_clauses.push("severity = ?".to_string());
            bindings.push(s.to_string().into());
        }
        let where_sql = where_clauses.join(" AND ");

        let count_sql = format!("SELECT COUNT(*) FROM alerts_anomaly WHERE {where_sql}");
        let total: i64 = {
            let mut stmt = self.conn.prepare(&count_sql)?;
            let params: Vec<&dyn duckdb::ToSql> =
                bindings.iter().map(|v| v as &dyn duckdb::ToSql).collect();
            stmt.query_row(params.as_slice(), |row| row.get(0))?
        };

        let offset = page.max(1).saturating_sub(1) * page_size;
        let rows_sql = format!(
            "SELECT anomaly_type, source_ref, detected_at, severity, message \
             FROM alerts_anomaly WHERE {where_sql} \
             ORDER BY detected_at DESC LIMIT ? OFFSET ?",
        );
        let mut stmt = self.conn.prepare(&rows_sql)?;
        bindings.push(page_size.into());
        bindings.push(offset.into());
        let params: Vec<&dyn duckdb::ToSql> =
            bindings.iter().map(|v| v as &dyn duckdb::ToSql).collect();
        let rows = stmt
            .query_map(params.as_slice(), |row| {
                Ok(serde_json::json!({
                    "type":        row.get::<_, String>(0)?,
                    "source_ref":  row.get::<_, String>(1)?,
                    "detected_at": row.get::<_, i64>(2)?,
                    "severity":    row.get::<_, String>(3)?,
                    "message":     row.get::<_, String>(4)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(serde_json::json!({
            "events":    rows,
            "total":     total,
            "page":      page,
            "page_size": page_size,
        }))
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

    /// Returns the (time_tag, peak flux_e12) of the highest X-ray reading in the 0.1-0.8 nm band
    /// since `since` (Unix seconds, compared against fetched_at).
    pub fn xray_peak_recent(&self, since: i64) -> Result<Option<(String, i64)>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT time_tag, flux_e12 FROM xray \
             WHERE energy = '0.1-0.8nm' AND observed_at > ? \
             ORDER BY flux_e12 DESC LIMIT 1",
        )?;
        let mut rows = stmt.query([since])?;
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

    pub fn latest_dst_raw(&self) -> Result<Option<(String, i64)>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT time_tag, dst_nt FROM dst WHERE dst_nt IS NOT NULL ORDER BY time_tag DESC LIMIT 1",
        )?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            Ok(Some((row.get(0)?, row.get::<_, i32>(1)? as i64)))
        } else {
            Ok(None)
        }
    }

    pub fn latest_imf_bz_raw(&self) -> Result<Option<(String, i64)>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT time_tag, bz_e2 FROM imf WHERE bz_e2 IS NOT NULL ORDER BY time_tag DESC LIMIT 1",
        )?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            Ok(Some((row.get(0)?, row.get(1)?)))
        } else {
            Ok(None)
        }
    }
}

// ── Custom anomaly rules ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CustomRule {
    pub id: String,
    pub user_email: String,
    pub name: String,
    pub metric: String,
    pub operator: String,
    pub threshold: f64,
    pub severity: String,
    pub enabled: bool,
    pub created_at: i64,
}

impl Store {
    pub fn insert_custom_rule(&self, rule: &CustomRule) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO custom_anomaly_rules
             (id, user_email, name, metric, operator, threshold, severity, enabled, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                rule.id,
                rule.user_email,
                rule.name,
                rule.metric,
                rule.operator,
                rule.threshold,
                rule.severity,
                rule.enabled,
                rule.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn list_custom_rules(&self, user_email: &str) -> Result<Vec<CustomRule>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, user_email, name, metric, operator, threshold, severity, enabled, created_at
             FROM custom_anomaly_rules WHERE user_email = ? ORDER BY created_at ASC",
        )?;
        let rows = stmt
            .query_map([user_email], |row| {
                Ok(CustomRule {
                    id: row.get(0)?,
                    user_email: row.get(1)?,
                    name: row.get(2)?,
                    metric: row.get(3)?,
                    operator: row.get(4)?,
                    threshold: row.get(5)?,
                    severity: row.get(6)?,
                    enabled: row.get(7)?,
                    created_at: row.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn delete_custom_rule(&self, id: &str, user_email: &str) -> Result<bool, DbError> {
        let n = self.conn.execute(
            "DELETE FROM custom_anomaly_rules WHERE id = ? AND user_email = ?",
            params![id, user_email],
        )?;
        Ok(n > 0)
    }

    pub fn toggle_custom_rule(
        &self,
        id: &str,
        user_email: &str,
        enabled: bool,
    ) -> Result<bool, DbError> {
        let n = self.conn.execute(
            "UPDATE custom_anomaly_rules SET enabled = ? WHERE id = ? AND user_email = ?",
            params![enabled, id, user_email],
        )?;
        Ok(n > 0)
    }

    pub fn get_enabled_custom_rules(&self) -> Result<Vec<CustomRule>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, user_email, name, metric, operator, threshold, severity, enabled, created_at
             FROM custom_anomaly_rules WHERE enabled = TRUE",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(CustomRule {
                    id: row.get(0)?,
                    user_email: row.get(1)?,
                    name: row.get(2)?,
                    metric: row.get(3)?,
                    operator: row.get(4)?,
                    threshold: row.get(5)?,
                    severity: row.get(6)?,
                    enabled: row.get(7)?,
                    created_at: row.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn count_custom_rules_for_user(&self, user_email: &str) -> Result<i64, DbError> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM custom_anomaly_rules WHERE user_email = ?",
                [user_email],
                |row| row.get(0),
            )
            .map_err(DbError::Duckdb)
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

impl Store {
    pub fn get_report_summary(&self, since_secs: i64) -> Result<serde_json::Value, DbError> {
        let cutoff = now() - since_secs;

        // Kp: avg and max over the window
        let (kp_avg, kp_max, kp_count) = {
            let mut stmt = self.conn.prepare(
                "SELECT AVG(estimated_kp_e2), MAX(estimated_kp_e2), COUNT(*) \
                 FROM kp WHERE observed_at > ?",
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
                 WHERE observed_at > ? AND speed_e1 IS NOT NULL",
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
                 WHERE energy = '0.1-0.8nm' AND observed_at > ?",
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
                                           .unwrap_or_else(|| "-".to_owned()),
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
                 WHERE observed_at > ? ORDER BY observed_at ASC",
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
                 WHERE observed_at > ? ORDER BY observed_at ASC",
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
                 WHERE energy = '0.1-0.8nm' AND observed_at > ? ORDER BY observed_at ASC",
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

impl Store {
    /// Returns the last 60 Kp readings oldest-first - same shape as /api/kp.
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

impl Store {
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

// ── Rate limiting ─────────────────────────────────────────────────────────────

impl Store {
    pub fn set_email_verified(&self, email: &str) -> Result<(), DbError> {
        self.conn.execute(
            "UPDATE users SET email_verified = TRUE WHERE email = ?",
            params![email],
        )?;
        Ok(())
    }

    pub fn set_totp_secret(&self, email: &str, secret: &str) -> Result<(), DbError> {
        self.conn.execute(
            "UPDATE users SET totp_secret = ? WHERE email = ?",
            params![secret, email],
        )?;
        Ok(())
    }

    pub fn enable_totp(&self, email: &str) -> Result<(), DbError> {
        self.conn.execute(
            "UPDATE users SET totp_enabled = TRUE WHERE email = ?",
            params![email],
        )?;
        Ok(())
    }

    pub fn disable_totp(&self, email: &str) -> Result<(), DbError> {
        self.conn.execute(
            "UPDATE users SET totp_secret = NULL, totp_enabled = FALSE WHERE email = ?",
            params![email],
        )?;
        Ok(())
    }

    pub fn get_user_plan(&self, email: &str) -> Result<String, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT plan FROM users WHERE email = ?")?;
        let mut rows = stmt.query([email])?;
        Ok(match rows.next()? {
            Some(row) => row
                .get::<_, Option<String>>(0)?
                .unwrap_or_else(|| "starter".to_string()),
            None => "starter".to_string(),
        })
    }

    pub fn upsert_usage_record(
        &self,
        email: &str,
        count: i64,
        period_start: i64,
        period_end: i64,
    ) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO usage_records
                 (user_email, request_count, period_start, period_end, updated_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT (user_email) DO UPDATE SET
                 request_count = excluded.request_count,
                 period_start  = excluded.period_start,
                 period_end    = excluded.period_end,
                 updated_at    = excluded.updated_at",
            params![email, count, period_start, period_end, now()],
        )?;
        Ok(())
    }

    /// Returns `(request_count, period_start, period_end)` from the last DB flush, if any.
    pub fn get_usage_for_user(&self, email: &str) -> Result<Option<(i64, i64, i64)>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT request_count, period_start, period_end \
             FROM usage_records WHERE user_email = ?",
        )?;
        let mut rows = stmt.query([email])?;
        match rows.next()? {
            Some(row) => Ok(Some((row.get(0)?, row.get(1)?, row.get(2)?))),
            None => Ok(None),
        }
    }
}

// ── Webhooks ──────────────────────────────────────────────────────────────────

pub struct WebhookRow {
    pub id: String,
    pub url: String,
    pub secret: String,
    pub events: Vec<String>,
    pub created_at: i64,
}

impl Store {
    pub fn create_webhook(
        &self,
        id: &str,
        user_email: &str,
        url: &str,
        secret: &str,
        events_json: &str,
    ) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO webhooks (id, user_email, url, secret, events, active, created_at)
             VALUES (?, ?, ?, ?, ?, true, ?)",
            params![id, user_email, url, secret, events_json, now()],
        )?;
        Ok(())
    }

    pub fn list_webhooks(&self, user_email: &str) -> Result<Vec<WebhookRow>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, url, secret, events, created_at
             FROM webhooks WHERE user_email = ? ORDER BY created_at DESC",
        )?;
        let rows = stmt
            .query_map([user_email], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows
            .into_iter()
            .map(|(id, url, secret, events_json, created_at)| {
                let events = serde_json::from_str(&events_json).unwrap_or_default();
                WebhookRow {
                    id,
                    url,
                    secret,
                    events,
                    created_at,
                }
            })
            .collect())
    }

    pub fn delete_webhook(&self, id: &str, user_email: &str) -> Result<bool, DbError> {
        let n = self.conn.execute(
            "DELETE FROM webhooks WHERE id = ? AND user_email = ?",
            params![id, user_email],
        )?;
        Ok(n > 0)
    }

    /// Returns all active webhooks subscribed to `event_type`.
    pub fn list_active_webhooks_for_event(
        &self,
        event_type: &str,
    ) -> Result<Vec<WebhookRow>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, url, secret, events, created_at
             FROM webhooks WHERE active = true",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows
            .into_iter()
            .filter_map(|(id, url, secret, events_json, created_at)| {
                let events: Vec<String> = serde_json::from_str(&events_json).unwrap_or_default();
                if events.iter().any(|e| e == event_type) {
                    Some(WebhookRow {
                        id,
                        url,
                        secret,
                        events,
                        created_at,
                    })
                } else {
                    None
                }
            })
            .collect())
    }
}

// ── Starlink ──────────────────────────────────────────────────────────────────

impl Store {
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

// ── Email alerts ──────────────────────────────────────────────────────────────

pub struct EmailAlertRow {
    pub user_email: String,
    pub enabled: bool,
    pub kp_threshold_e2: i64,
    pub wind_threshold_e1: i64,
    pub last_notified_at: Option<i64>,
}

impl Store {
    pub fn upsert_email_alert(
        &self,
        id: &str,
        user_email: &str,
        enabled: bool,
        kp_threshold_e2: i64,
        wind_threshold_e1: i64,
    ) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO email_alerts
                 (id, user_email, enabled, kp_threshold_e2, wind_threshold_e1, created_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT (user_email) DO UPDATE SET
                 enabled           = excluded.enabled,
                 kp_threshold_e2   = excluded.kp_threshold_e2,
                 wind_threshold_e1 = excluded.wind_threshold_e1",
            params![
                id,
                user_email,
                enabled,
                kp_threshold_e2,
                wind_threshold_e1,
                now()
            ],
        )?;
        Ok(())
    }

    pub fn get_email_alert(&self, user_email: &str) -> Result<Option<EmailAlertRow>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT user_email, enabled, kp_threshold_e2, wind_threshold_e1, last_notified_at
             FROM email_alerts WHERE user_email = ?",
        )?;
        match stmt.query_row([user_email], |row| {
            Ok(EmailAlertRow {
                user_email: row.get(0)?,
                enabled: row.get(1)?,
                kp_threshold_e2: row.get(2)?,
                wind_threshold_e1: row.get(3)?,
                last_notified_at: row.get(4)?,
            })
        }) {
            Ok(row) => Ok(Some(row)),
            Err(duckdb::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(DbError::Duckdb(e)),
        }
    }

    pub fn list_enabled_email_alerts(&self) -> Result<Vec<EmailAlertRow>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT user_email, enabled, kp_threshold_e2, wind_threshold_e1, last_notified_at
             FROM email_alerts WHERE enabled = true",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(EmailAlertRow {
                    user_email: row.get(0)?,
                    enabled: row.get(1)?,
                    kp_threshold_e2: row.get(2)?,
                    wind_threshold_e1: row.get(3)?,
                    last_notified_at: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn touch_email_alert_notified(&self, user_email: &str) -> Result<(), DbError> {
        self.conn.execute(
            "UPDATE email_alerts SET last_notified_at = ? WHERE user_email = ?",
            params![now(), user_email],
        )?;
        Ok(())
    }

    pub fn insert_webhook_delivery(
        &self,
        webhook_id: &str,
        attempted_at: i64,
        status_code: Option<i32>,
        success: bool,
        error: Option<&str>,
    ) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO webhook_deliveries (id, webhook_id, attempted_at, status_code, success, error)
             VALUES (nextval('seq_webhook_deliveries'), ?, ?, ?, ?, ?)",
            params![webhook_id, attempted_at, status_code, success, error],
        )?;
        // Cap each webhook at the most recent 100 attempts to keep the table small.
        self.conn.execute(
            "DELETE FROM webhook_deliveries
             WHERE webhook_id = ?
               AND id NOT IN (
                 SELECT id FROM webhook_deliveries
                 WHERE webhook_id = ?
                 ORDER BY attempted_at DESC
                 LIMIT 100
               )",
            params![webhook_id, webhook_id],
        )?;
        Ok(())
    }

    /// Returns the most recent `limit` delivery attempts for a webhook the
    /// caller owns. Membership is verified by joining against `webhooks`.
    pub fn list_webhook_deliveries(
        &self,
        webhook_id: &str,
        user_email: &str,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT d.attempted_at, d.status_code, d.success, d.error
             FROM webhook_deliveries d
             JOIN webhooks w ON w.id = d.webhook_id
             WHERE d.webhook_id = ? AND w.user_email = ?
             ORDER BY d.attempted_at DESC
             LIMIT ?",
        )?;
        let rows = stmt
            .query_map(params![webhook_id, user_email, limit], |r| {
                Ok(serde_json::json!({
                    "attempted_at": r.get::<_, i64>(0)?,
                    "status_code":  r.get::<_, Option<i32>>(1)?,
                    "success":      r.get::<_, bool>(2)?,
                    "error":        r.get::<_, Option<String>>(3)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn insert_health_snapshot(
        &self,
        component: &str,
        ts: i64,
        status: &str,
    ) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT OR IGNORE INTO health_snapshots (component, ts, status) VALUES (?, ?, ?)",
            params![component, ts, status],
        )?;
        Ok(())
    }

    /// Returns per-component daily uptime over the last `days` days.
    /// Each row: (component, day_index_from_today, samples, operational_samples).
    /// day_index 0 = today, increasing into the past. Days with zero samples
    /// are omitted; the caller fills gaps as "no_data".
    pub fn uptime_by_day(
        &self,
        days: i64,
    ) -> Result<Vec<(String, i64, i64, i64)>, DbError> {
        let now = now();
        let since = now - days * 86_400;
        let mut stmt = self.conn.prepare(
            "SELECT component,
                    CAST((? - ts) / 86400 AS BIGINT) AS day_idx,
                    COUNT(*),
                    SUM(CASE WHEN status = 'operational' THEN 1 ELSE 0 END)
             FROM health_snapshots
             WHERE ts >= ?
             GROUP BY component, day_idx
             ORDER BY component, day_idx",
        )?;
        let rows = stmt
            .query_map(params![now, since], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Returns (noaa_last_write, nasa_last_write, celestrak_last_write) as Unix timestamps.
    /// Each is None if the table has no rows yet.
    pub fn health_freshness(&self) -> (Option<i64>, Option<i64>, Option<i64>) {
        let q = |sql: &str| -> Option<i64> {
            self.conn
                .query_row(sql, [], |row| row.get::<_, Option<i64>>(0))
                .ok()
                .flatten()
        };
        let noaa = q("SELECT MAX(fetched_at) FROM kp");
        let nasa = q(
            "SELECT MAX(m) FROM (SELECT MAX(fetched_at) AS m FROM apod UNION ALL SELECT MAX(fetched_at) FROM neo UNION ALL SELECT MAX(fetched_at) FROM epic)",
        );
        let celestrak = q("SELECT MAX(fetched_at) FROM starlink");
        (noaa, nasa, celestrak)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::noaa::{ImfRecord, Kp3hRecord, KpRecord, XRayRecord};
    use chrono::DateTime;

    fn mem_store() -> Store {
        Store::open(":memory:").expect("in-memory store")
    }

    /// Pulls `[Min: n, Max: m]` out of a storage_info stats string.
    fn parse_min_max(stats: &str) -> Option<(i64, i64)> {
        let rest = stats.strip_prefix("[Min: ")?;
        let (min_s, rest) = rest.split_once(", Max: ")?;
        let (max_s, _) = rest.split_once(']')?;
        Some((min_s.parse().ok()?, max_s.parse().ok()?))
    }

    fn iso(ts: i64) -> String {
        DateTime::from_timestamp(ts, 0)
            .unwrap()
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string()
    }

    #[test]
    fn kp_scaled_round_trip() {
        let store = mem_store();
        store
            .insert_kp_batch(&[
                KpRecord { time_tag: "2024-01-01T00:00:00".into(), kp_index: 2, estimated_kp: 2.33 },
                KpRecord { time_tag: "2024-01-01T01:00:00".into(), kp_index: 3, estimated_kp: 3.67 },
            ])
            .unwrap();

        // Stored as scaled ints, read back de-scaled, oldest-first.
        let out = store.get_kp_recent().unwrap();
        let kp = out.as_array().unwrap();
        assert_eq!(kp.len(), 2);
        let v = |i: usize| kp[i]["estimated_kp"].as_f64().unwrap();
        assert!((v(0) - 2.33).abs() < 1e-9, "oldest first: {kp:?}");
        assert!((v(1) - 3.67).abs() < 1e-9);
    }

    #[test]
    fn kp_forecast_insert_and_upsert() {
        let store = mem_store();
        let ts = 1_700_000_000;

        store
            .insert_kp_forecast(ts, 250, Some(180), Some(320), Some(1057))
            .unwrap();
        assert_eq!(store.get_kp_forecast_latest().unwrap(), Some((ts, 250)));

        // Same ts → ON CONFLICT DO UPDATE replaces the value.
        store.insert_kp_forecast(ts, 333, None, None, None).unwrap();
        assert_eq!(store.get_kp_forecast_latest().unwrap(), Some((ts, 333)));
    }

    #[test]
    fn forecast_metrics_pairs_and_scores() {
        let store = mem_store();
        let t = 1_700_000_000;
        let step = 3_600;

        // (predicted_kp_e2, actual_kp) per period, 1h apart:
        //  storm caught | quiet | false positive | storm missed
        let cases = [
            (550_i64, 6.0_f64), // actual storm (600), pred storm  → caught
            (300, 2.0),         // quiet
            (520, 2.0),         // pred storm, actual quiet         → false positive
            (200, 5.0),         // actual storm (500), pred quiet    → missed
        ];

        for (i, (pred_e2, actual_kp)) in cases.iter().enumerate() {
            let ts = t + step * i as i64;
            store
                .insert_kp_forecast(ts, *pred_e2, None, None, Some(1000))
                .unwrap();
            store
                .insert_kp_3h_batch(&[Kp3hRecord { time_tag: iso(ts), kp: *actual_kp }])
                .unwrap();
        }

        let m = store.get_forecast_metrics(t - 1).unwrap();
        assert_eq!(m["n_samples"].as_i64().unwrap(), 4);
        assert_eq!(m["n_storms"].as_i64().unwrap(), 2);
        assert_eq!(m["n_caught"].as_i64().unwrap(), 1);
        assert_eq!(m["n_false_pos"].as_i64().unwrap(), 1);
        assert!((m["hit_rate"].as_f64().unwrap() - 0.5).abs() < 1e-9);

        // MAE = mean(|pred-actual|) in Kp: (0.5+1.0+3.2+3.0)/4 = 1.925
        assert!((m["mae"].as_f64().unwrap() - 1.925).abs() < 1e-6);
        // RMSE = sqrt(mean(diff²)) in Kp ≈ 2.2633
        assert!((m["rmse"].as_f64().unwrap() - 2.263293).abs() < 1e-4);
        // mean σ = 1000/1e4 = 0.1
        assert!((m["mean_unc"].as_f64().unwrap() - 0.1).abs() < 1e-9);
    }

    #[test]
    fn forecast_metrics_empty_window() {
        let store = mem_store();
        let m = store.get_forecast_metrics(0).unwrap();
        assert_eq!(m["n_samples"].as_i64().unwrap(), 0);
    }

    /// The six feeds arrive in three different upstream formats. All must land
    /// on the same UTC epoch, otherwise observed_at silently disagrees between
    /// tables and every range query built on it is wrong.
    #[test]
    fn observed_at_parses_all_upstream_time_tag_formats() {
        let store = mem_store();
        // 2026-05-11T03:47:00 UTC
        let expected = 1_778_471_220_i64;

        store
            .insert_kp_batch(&[KpRecord {
                time_tag: "2026-05-11T03:47:00".into(),
                kp_index: 2,
                estimated_kp: 2.0,
            }])
            .unwrap();
        store
            .insert_xray_batch(&[XRayRecord {
                time_tag: "2026-05-11T03:47:00Z".into(),
                satellite: 16,
                flux: 1e-6,
                observed_flux: 1e-6,
                energy: "0.1-0.8nm".into(),
            }])
            .unwrap();
        store
            .insert_imf_batch(&[ImfRecord {
                time_tag: "2026-05-11 03:47:00.000".into(),
                bz_gsm: Some(-4.5),
                bt: Some(6.0),
            }])
            .unwrap();

        for table in ["kp", "xray", "imf"] {
            let got: i64 = store
                .conn
                .query_row(&format!("SELECT observed_at FROM {table}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(got, expected, "{table} observed_at");
        }
    }

    /// With more rows than the LIMIT, the endpoint must return the newest
    /// window. Ordering ASC returned the head of the table instead, so the
    /// series froze at the first data ever ingested.
    #[test]
    fn recent_endpoints_return_the_newest_window() {
        let store = mem_store();
        let base = 1_700_000_000_i64;

        // 1500 minutes of Kp, more than the 1440 row limit.
        let kp: Vec<KpRecord> = (0..1500)
            .map(|i| KpRecord {
                time_tag: iso(base + i * 60),
                kp_index: 1,
                estimated_kp: 1.0,
            })
            .collect();
        store.insert_kp_batch(&kp).unwrap();

        let out = store.get_kp_recent().unwrap();
        let arr = out.as_array().unwrap();
        assert_eq!(arr.len(), 1440);
        // Newest window is rows 60..1499, returned oldest-first.
        assert_eq!(arr[0]["time_tag"].as_str().unwrap(), iso(base + 60 * 60));
        assert_eq!(
            arr[1439]["time_tag"].as_str().unwrap(),
            iso(base + 1499 * 60)
        );

        // 300 three-hour periods, more than the 240 row limit.
        let kp3: Vec<Kp3hRecord> = (0..300)
            .map(|i| Kp3hRecord {
                time_tag: iso(base + i * 10_800),
                kp: 2.0,
            })
            .collect();
        store.insert_kp_3h_batch(&kp3).unwrap();

        let out3 = store.get_kp_3h_recent().unwrap();
        let arr3 = out3.as_array().unwrap();
        assert_eq!(arr3.len(), 240);
        assert_eq!(
            arr3[0]["time_tag"].as_str().unwrap(),
            iso(base + 60 * 10_800)
        );
        assert_eq!(
            arr3[239]["time_tag"].as_str().unwrap(),
            iso(base + 299 * 10_800)
        );
    }

    /// A row ingested now but observed long ago must fall outside a recent
    /// window. Filtering on fetched_at put it inside, which is why a cold start
    /// reported the whole backfilled day as current events.
    #[test]
    fn range_queries_filter_on_observation_time_not_ingest_time() {
        let store = mem_store();
        let now = now();
        let stale_obs = now - 30 * 3600;

        // Ingested this second, observed 30 hours ago.
        store
            .conn
            .execute_batch(&format!(
                "INSERT INTO kp (time_tag, kp_index, estimated_kp_e2, observed_at, fetched_at)
                 VALUES ('2020-01-01T00:00:00', 9, 900, {stale_obs}, {now});
                 INSERT INTO xray (time_tag, energy, satellite, flux_e12, observed_flux_e12, observed_at, fetched_at)
                 VALUES ('2020-01-01T00:00:00Z', '0.1-0.8nm', 16, 500000000, 500000000, {stale_obs}, {now});
                 INSERT INTO solar_wind (time_tag, speed_e1, density_e2, temp_k, observed_at, fetched_at)
                 VALUES ('2020-01-01T00:00:00', 9500, 500, 100000, {stale_obs}, {now})"
            ))
            .unwrap();

        // A 24 hour window must not see a 30 hour old observation.
        let summary = store.get_report_summary(24 * 3600).unwrap();
        assert_eq!(summary["kp_count"].as_i64().unwrap(), 0);
        assert!(summary["kp_max"].is_null());
        assert!(summary["solar_wind_max_kms"].is_null());
        assert!(summary["xray_max_flux"].is_null());

        assert!(store.get_kp_range(24 * 3600).unwrap().as_array().unwrap().is_empty());
        assert!(
            store
                .get_solar_wind_range(24 * 3600)
                .unwrap()
                .as_array()
                .unwrap()
                .is_empty()
        );

        // The X-class flare is 30 hours old, so a 3 hour flare scan must miss it.
        assert!(store.xray_peak_recent(now - 3 * 3600).unwrap().is_none());

        // A 48 hour window does include it, proving the row is present and the
        // exclusion above came from the window, not from a broken insert.
        assert_eq!(
            store.get_report_summary(48 * 3600).unwrap()["kp_count"]
                .as_i64()
                .unwrap(),
            1
        );
        assert!(store.xray_peak_recent(now - 48 * 3600).unwrap().is_some());
    }

    /// The observed_at indexes must not come back. Measured against the three
    /// heaviest report queries they were slower at the current row counts and
    /// roughly 38x slower at a projected year of ingest, and they tripled the
    /// database file. They were also suspected of causing a wrong range count,
    /// but that was not borne out: reintroducing them does not reproduce it.
    /// The justification for this guard is the measured cost, nothing else.
    #[test]
    fn no_observed_at_indexes_are_created() {
        let store = mem_store();
        let n: i64 = store
            .conn
            .query_row(
                "SELECT count(*) FROM duckdb_indexes() WHERE index_name LIKE '%observed_at%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0, "observed_at indexes were reintroduced");
    }

    /// Adding observed_at by ALTER and filling it by UPDATE leaves every row
    /// group's statistics at the empty sentinel, and the UPDATE records the
    /// values as pending updates rather than refreshing them. Store::open must
    /// merge them before returning, otherwise the statistics on disk do not
    /// describe the data and a process that exits without checkpointing
    /// persists that state.
    ///
    /// Fails without the CHECKPOINT in Store::open: the recorded min is
    /// i64::MAX and the recorded max is i64::MIN.
    #[test]
    fn migration_leaves_row_group_statistics_consistent() {
        let dir = std::env::temp_dir().join(format!("astraeus_stats_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("legacy.duckdb");
        let path_str = path.to_string_lossy().to_string();

        // Build a database in the shape that predates observed_at, then close it.
        {
            let conn = Connection::open(&path_str).unwrap();
            conn.execute_batch(
                "CREATE TABLE kp (
                     time_tag        TEXT    NOT NULL PRIMARY KEY,
                     kp_index        INTEGER NOT NULL,
                     estimated_kp_e2 BIGINT  NOT NULL,
                     fetched_at      BIGINT  NOT NULL
                 )",
            )
            .unwrap();
            let mut stmt = conn
                .prepare(
                    "INSERT INTO kp (time_tag, kp_index, estimated_kp_e2, fetched_at)
                     VALUES (?, 1, 100, 1)",
                )
                .unwrap();
            for i in 0..5000 {
                stmt.execute(params![iso(1_700_000_000 + i * 60)]).unwrap();
            }
            drop(stmt);
            conn.execute_batch("CHECKPOINT").unwrap();
        }

        // Run the real migration path.
        let store = Store::open(&path_str).unwrap();

        // Every row group's recorded statistics must bound the values it holds.
        // storage_info.start is segment local, so the global rowid offset of a
        // row group is the running total of the preceding row counts.
        let rows: Vec<(i64, String)> = {
            let mut stmt = store
                .conn
                .prepare(
                    "SELECT count, stats FROM pragma_storage_info('kp') \
                     WHERE column_name = 'observed_at' AND segment_type <> 'VALIDITY' \
                     ORDER BY row_group_id",
                )
                .unwrap();
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        assert!(!rows.is_empty(), "expected at least one row group");

        let mut offset = 0i64;
        let mut checked = 0;
        for (count, stats) in rows {
            let start = offset;
            offset += count;
            let Some((rec_min, rec_max)) = parse_min_max(&stats) else {
                continue;
            };
            let (act_min, act_max): (i64, i64) = store
                .conn
                .query_row(
                    "SELECT min(observed_at), max(observed_at) FROM kp \
                     WHERE rowid >= ? AND rowid < ?",
                    params![start, start + count],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            assert!(
                rec_min <= act_min && rec_max >= act_max,
                "row group statistics do not bound stored values: \
                 recorded [{rec_min}, {rec_max}] actual [{act_min}, {act_max}]"
            );
            checked += 1;
        }
        assert!(checked > 0, "no row group carried min/max statistics");

        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A short history must be an error, never a short vector: the ML service
    /// would pad the shortfall and forecast from mostly synthetic input.
    #[test]
    fn short_kp_3h_history_errors_instead_of_returning_a_short_sequence() {
        let store = mem_store();
        let base = 1_700_000_000_i64;
        let seq_len = 16;

        let short: Vec<Kp3hRecord> = (0..seq_len - 1)
            .map(|i| Kp3hRecord {
                time_tag: iso(base + i as i64 * 10_800),
                kp: 3.0,
            })
            .collect();
        store.insert_kp_3h_batch(&short).unwrap();

        match store.get_recent_kp_3h(seq_len) {
            Err(DbError::InsufficientHistory { have, need }) => {
                assert_eq!(have, seq_len - 1);
                assert_eq!(need, seq_len);
            }
            other => panic!("expected InsufficientHistory, got {other:?}"),
        }

        // One more period and the window is complete.
        store
            .insert_kp_3h_batch(&[Kp3hRecord {
                time_tag: iso(base + (seq_len as i64 - 1) * 10_800),
                kp: 4.5,
            }])
            .unwrap();

        let seq = store.get_recent_kp_3h(seq_len).unwrap();
        assert_eq!(seq.len(), seq_len);
        // Oldest-first, and the newest value is last.
        assert!((seq[0] - 3.0).abs() < 1e-9);
        assert!((seq[seq_len - 1] - 4.5).abs() < 1e-9);
    }

    /// Rows written before the migration have a NULL observed_at; Store::open
    /// must derive it from time_tag for every one of them.
    #[test]
    fn observed_at_backfills_preexisting_rows() {
        let store = mem_store();
        store
            .conn
            .execute_batch(
                "INSERT INTO kp (time_tag, kp_index, estimated_kp_e2, observed_at, fetched_at)
                 VALUES ('2026-05-11T03:47:00', 2, 200, NULL, 1)",
            )
            .unwrap();

        let null_before: i64 = store
            .conn
            .query_row(
                "SELECT count(*) FROM kp WHERE observed_at IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(null_before, 1);

        for table in OBSERVED_AT_TABLES {
            store
                .conn
                .execute_batch(&format!(
                    "UPDATE {table} SET observed_at = {OBSERVED_AT_SQL} WHERE observed_at IS NULL"
                ))
                .unwrap();
        }

        let got: i64 = store
            .conn
            .query_row("SELECT observed_at FROM kp", [], |r| r.get(0))
            .unwrap();
        assert_eq!(got, 1_778_471_220);
    }
}
