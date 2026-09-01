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
    #[error("{0}")]
    EncryptionKey(#[from] crate::secretbox::KeyError),
    #[error(
        "TOTP_ENCRYPTION_KEY is not set but {count} account(s) have an encrypted second factor.          Refusing to start: without the key those accounts cannot sign in, and 2FA could be          silently turned off underneath them. Set the key, or clear totp_enabled and          totp_secret_enc for those accounts to disable 2FA deliberately."
    )]
    EncryptionKeyMissing { count: i64 },
    #[error(
        "TOTP_ENCRYPTION_KEY does not decrypt the stored second factors. Refusing to start:          this is the wrong key, not a missing one. Restore the original key."
    )]
    EncryptionKeyWrong,
    #[error("second factor storage is not available")]
    EncryptionUnavailable,
    #[error("insufficient Kp history: have {have} three-hour readings, need {need}")]
    InsufficientHistory { have: usize, need: usize },
    /// The message reaches the reader as the body of the failed forecast, so it
    /// says what happened rather than naming the table. `series` and
    /// `newest_observed_at` are for the log line.
    #[error("Kp history is not up to date. The forecast will return when new readings arrive.")]
    StaleSeries {
        series: &'static str,
        newest_observed_at: Option<i64>,
    },
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
    PRIMARY KEY (time_tag, energy, satellite)
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

/// One series and how old its newest observation may be before it counts as
/// stale.
pub struct SeriesFreshness {
    /// Name this series reports under on the status page.
    pub component: &'static str,
    pub table: &'static str,
    /// Column holding the observation time as Unix seconds. The NOAA series
    /// derive `observed_at` from an ISO text `time_tag`; `iss_position` is
    /// already keyed on an epoch, so it has no such column.
    pub time_column: &'static str,
    pub max_age_secs: i64,
}

/// A component whose health is a statement about our polling rather than about
/// the age of the data it stores.
///
/// `SERIES_FRESHNESS` answers "is the newest row recent enough", which is the
/// right question for a feed that publishes on a timetable and the wrong one
/// for a feed that publishes when something happens. The NOAA alerts feed is
/// episodic: days of silence are correct behaviour, so no row age separates
/// quiet from dead, and it therefore sat with no entry in either direction and
/// no component of its own on the status page.
///
/// What can be asserted about such a feed is our own poll: it ran, it succeeded,
/// and it came back with something. The poller writes that verdict into
/// `health_snapshots` each cycle and this table says how stale that verdict may
/// get before it stops being believed, which is what stops a stopped poller
/// repeating its last good answer forever.
pub struct PollLiveness {
    pub component: &'static str,
    /// How old the newest recorded verdict may be. Comfortably several poll
    /// intervals, so an ordinary restart or one missed cycle does not show as a
    /// fault.
    pub max_verdict_age_secs: i64,
}

/// Every component reported from a poll verdict rather than from row age.
///
/// Declared, like `SERIES_FRESHNESS`, so that the status page, `/api/health` and
/// `component-check.sh` enumerate the same set. A list built from what has
/// already spoken is how a dead feed stays invisible.
pub const POLL_LIVENESS: [PollLiveness; 1] = [PollLiveness {
    // Polls every 300 s; six missed cycles is a fault and one is not.
    component: "noaa_alerts",
    max_verdict_age_secs: 1_800,
}];

/// Components that are monitored but do not decide whether the product works.
///
/// The NASA feeds are interesting rather than load bearing: an astronomy
/// picture failing to fetch says nothing about space weather. They kept their
/// freshness entries, their health snapshots, their uptime history and their
/// cron mail; what they lost is the ability to put "degraded" at the top of the
/// public status page, where a satellite operator reads it as our space weather
/// data being broken.
///
/// Declared as an exclusion rather than an inclusion on purpose. A component
/// added later counts toward `status` unless somebody names it here, so the
/// default is that new things matter and hiding one is a deliberate act with a
/// name attached.
pub const AUXILIARY: [&str; 4] = ["nasa_apod", "nasa_epic", "nasa_neo", "nasa_exoplanets"];

/// Whether a component is excluded from the overall status.
pub fn is_auxiliary(component: &str) -> bool {
    AUXILIARY.contains(&component)
}

/// Whether a reading is recent enough to describe conditions now.
///
/// The limit is the series' own `SERIES_FRESHNESS` entry rather than a second
/// number, so raising the tolerance for a feed raises it everywhere at once.
/// An unknown component is not fresh: a caller asking about something this
/// table has never heard of gets the safe answer.
pub fn reading_is_current(component: &str, observed_at: i64, now: i64) -> bool {
    SERIES_FRESHNESS
        .iter()
        .find(|s| s.component == component)
        .is_some_and(|s| now - observed_at <= s.max_age_secs)
}

/// Whether every component that decides the product's status is operational.
///
/// Pulled out of the health handler so the exclusion is testable on its own.
/// Left inline it was unguarded: a test can assert `is_auxiliary` and assert
/// that the feeds are still published without ever asserting that the filter
/// runs, which is what mutation testing found on 2026-09-01 within a minute of
/// the harness existing.
pub fn all_product_components_operational(
    components: &[(&'static str, &'static str, Option<i64>)],
) -> bool {
    components
        .iter()
        .filter(|(component, _, _)| !is_auxiliary(component))
        .all(|(_, status, _)| *status == "operational")
}

/// The freshness limit for every series, used by both the read path and the
/// status page so the two cannot disagree about what current means.
///
/// A series past its limit is not drawn and reports degraded. Before this
/// existed, the imf feed was dead for forty days while the charts kept drawing
/// its last day of data and the status page stayed green, because one Kp query
/// stood in for the whole of NOAA.
pub const SERIES_FRESHNESS: [SeriesFreshness; 11] = [
    SeriesFreshness {
        component: "noaa_kp",
        table: "kp",
        time_column: "observed_at",
        max_age_secs: 1_800,
    },
    // NOAA publishes this series about three hours after the period it covers,
    // so the newest stored value is normally between three and six hours old.
    // Six hours brushed the boundary before every new value arrived: the series
    // flipped to degraded for the last stretch of each cycle and, because the
    // forecast refuses a stale input, took the forecast down with it for roughly
    // thirty minutes in every three hours. Nine hours clears a full missed
    // publication plus the lag and still catches a dead feed within one cycle.
    SeriesFreshness {
        component: "noaa_kp_3h",
        table: "kp_3h",
        time_column: "observed_at",
        max_age_secs: 32_400,
    },
    SeriesFreshness {
        component: "noaa_solar_wind",
        table: "solar_wind",
        time_column: "observed_at",
        max_age_secs: 1_800,
    },
    SeriesFreshness {
        component: "noaa_xray",
        table: "xray",
        time_column: "observed_at",
        max_age_secs: 1_800,
    },
    SeriesFreshness {
        component: "noaa_imf",
        table: "imf",
        time_column: "observed_at",
        max_age_secs: 1_800,
    },
    // Kyoto publishes provisional Dst a day or more after the hour it covers,
    // so a limit near the others would report degraded every day with nothing
    // wrong. 36 hours is the smallest limit that clears the normal lag.
    SeriesFreshness {
        component: "noaa_dst",
        time_column: "observed_at",
        table: "dst",
        max_age_secs: 129_600,
    },
    // The ISS poll runs every five seconds, so anything older than five minutes
    // means the feed has stopped. Without an entry here a frozen position drew
    // the station parked at one point with nothing saying it was stale.
    SeriesFreshness {
        component: "iss",
        table: "iss_position",
        time_column: "ts",
        max_age_secs: 300,
    },
    // The NASA feeds below reported as one aggregate "nasa" component, taking
    // MAX(fetched_at) across apod, neo and epic. One live feed stood in for all
    // three, so APOD arriving daily kept the component green with NEO and EPIC
    // both dead. That is the same fault that let a single Kp query stand in for
    // the whole of NOAA, fixed the same way: one entry each.
    //
    // These key on `fetched_at` rather than an observation time because that is
    // what the tables carry. For it to mean "the poller is still working" the
    // inserts had to move from ON CONFLICT DO NOTHING to DO UPDATE of
    // `fetched_at`; previously it only advanced when a genuinely new row
    // appeared, so a quiet week in the exoplanet archive was indistinguishable
    // from a dead poller. Now it is the time of the last successful poll that
    // returned rows, and an empty payload correctly fails to advance it.
    SeriesFreshness {
        component: "nasa_apod",
        table: "apod",
        time_column: "fetched_at",
        max_age_secs: 10_800,
    },
    SeriesFreshness {
        component: "nasa_neo",
        table: "neo",
        time_column: "fetched_at",
        max_age_secs: 7_200,
    },
    SeriesFreshness {
        component: "nasa_epic",
        table: "epic",
        time_column: "fetched_at",
        max_age_secs: 7_200,
    },
    // Polled once a day, so the limit has to clear a full cycle plus a retry.
    SeriesFreshness {
        component: "nasa_exoplanets",
        table: "exoplanet",
        time_column: "fetched_at",
        max_age_secs: 172_800,
    },
];

/// Identifier for the one-shot purge of forecasts generated before the model
/// input was corrected to read the three-hour series.
const PURGE_FORECASTS_MIGRATION: &str = "2026-08-purge-kp-forecast-wrong-input-series";

/// Identifier for the one-shot removal of the observed_at indexes.
const DROP_OBSERVED_AT_INDEXES_MIGRATION: &str = "2026-08-drop-observed-at-indexes";

/// Adds `users.token_version`, the counter that lets a password change or reset
/// invalidate sessions that were issued before it.
const EMAIL_LOWERCASE_MIGRATION: &str = "2026-09-01-email-lowercase";
const TOKEN_VERSION_MIGRATION: &str = "2026-08-users-token-version";

/// Adds `api_keys.expires_at` and `api_keys.revoked_at`. Both nullable, so an
/// existing key stays unexpiring and unrevoked, which is the behaviour it had.
const API_KEY_LIFECYCLE_MIGRATION: &str = "2026-08-api-keys-lifecycle";

/// Adds `alerts_anomaly.user_email` and recovers the owner of rows already
/// written by a custom rule. NULL means the anomaly is global.
const ANOMALY_OWNER_MIGRATION: &str = "2026-08-alerts-anomaly-user-email";

/// Moves TOTP secrets from a plaintext column to an encrypted one and drops the
/// plaintext column. A column that sometimes holds a plaintext bearer credential
/// is the ambiguity that causes the next leak.
const TOTP_ENCRYPTION_MIGRATION: &str = "2026-08-encrypt-totp-secrets";

/// Moves custom rule thresholds from DOUBLE to the metric's own scaled integer,
/// so a reading exactly on the threshold compares exactly.
const RULE_THRESHOLD_MIGRATION: &str = "2026-08-scale-custom-rule-thresholds";

/// Rebuilds usage_records so its key is (user_email, period_start) instead of
/// (user_email). One row per user meant the previous period was overwritten by
/// the current one, so no billing history existed.
const USAGE_HISTORY_MIGRATION: &str = "2026-08-usage-records-history";

/// Rebuilds xray with satellite in the primary key. Without it, two satellites
/// reporting the same minute and band collided and one reading was silently
/// dropped by ON CONFLICT DO NOTHING.
const XRAY_SATELLITE_KEY_MIGRATION: &str = "2026-08-xray-satellite-in-primary-key";

/// Retires the `starter` tier. It existed only in the backend as the default for
/// a new account, ranked and priced identically to `free`, and the pricing page
/// never sold it, so the frontend had to translate it away on every render.
const RETIRE_STARTER_MIGRATION: &str = "2026-08-retire-starter-tier";

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
    /// None when no key is configured, which is allowed only while no account
    /// has an encrypted second factor. `Store::open` enforces that.
    secret_box: Option<crate::secretbox::SecretBox>,
}

impl Store {
    pub fn open(path: &str) -> Result<Self, DbError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        // Migrate existing DBs that pre-date the plan column.
        for sql in [
            "ALTER TABLE users ADD COLUMN plan TEXT DEFAULT 'free'",
            "ALTER TABLE users ADD COLUMN email_verified BOOLEAN DEFAULT FALSE",
            "ALTER TABLE users ADD COLUMN totp_secret TEXT",
            "ALTER TABLE users ADD COLUMN totp_enabled BOOLEAN DEFAULT FALSE",
            "ALTER TABLE users ADD COLUMN auth_provider TEXT DEFAULT 'password'",
            "ALTER TABLE users ADD COLUMN token_version BIGINT DEFAULT 0",
            "ALTER TABLE api_keys ADD COLUMN expires_at BIGINT",
            "ALTER TABLE api_keys ADD COLUMN revoked_at BIGINT",
            "ALTER TABLE alerts_anomaly ADD COLUMN user_email TEXT",
            "ALTER TABLE users ADD COLUMN totp_secret_enc TEXT",
            "ALTER TABLE custom_anomaly_rules ADD COLUMN threshold_scaled BIGINT",
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

        // token_version is added by the ALTER list above, which DuckDB fills
        // with the default for existing rows. This records that the step ran so
        // the migration history names every schema change, not just the ones
        // that also move data.
        let token_version_applied: i64 = conn.query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE id = ?",
            params![TOKEN_VERSION_MIGRATION],
            |row| row.get(0),
        )?;
        if token_version_applied == 0 {
            conn.execute(
                "INSERT INTO schema_migrations (id, applied_at) VALUES (?, ?)",
                params![TOKEN_VERSION_MIGRATION, now()],
            )?;
            info!("added users.token_version");
        }

        // Addresses are stored and compared in lower case from 2026-09-01. Six
        // production rows were already lower case when this was written, so
        // this is defensive rather than corrective, and it is written to fail
        // loudly rather than silently merge if two rows ever differ only in
        // case: `email` is the primary key, so a collision aborts the update
        // and the operator has to decide which account survives.
        let email_case_applied: i64 = conn.query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE id = ?",
            params![EMAIL_LOWERCASE_MIGRATION],
            |row| row.get(0),
        )?;
        if email_case_applied == 0 {
            let mixed: i64 = conn.query_row(
                "SELECT COUNT(*) FROM users WHERE email <> lower(email)",
                [],
                |row| row.get(0),
            )?;
            if mixed > 0 {
                conn.execute(
                    "UPDATE users SET email = lower(email) WHERE email <> lower(email)",
                    [],
                )?;
                info!("folded {mixed} account addresses to lower case");
            }
            conn.execute(
                "INSERT INTO schema_migrations (id, applied_at) VALUES (?, ?)",
                params![EMAIL_LOWERCASE_MIGRATION, now()],
            )?;
        }

        let api_key_lifecycle_applied: i64 = conn.query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE id = ?",
            params![API_KEY_LIFECYCLE_MIGRATION],
            |row| row.get(0),
        )?;
        if api_key_lifecycle_applied == 0 {
            conn.execute(
                "INSERT INTO schema_migrations (id, applied_at) VALUES (?, ?)",
                params![API_KEY_LIFECYCLE_MIGRATION, now()],
            )?;
            info!("added api_keys.expires_at and api_keys.revoked_at");
        }

        // Rows written by a custom rule carry the rule id in `anomaly_type` as
        // `custom:{id}`, so the owner is recoverable from custom_anomaly_rules.
        // Everything else was a global detection and stays NULL.
        let anomaly_owner_applied: i64 = conn.query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE id = ?",
            params![ANOMALY_OWNER_MIGRATION],
            |row| row.get(0),
        )?;
        if anomaly_owner_applied == 0 {
            let recovered = conn.execute(
                "UPDATE alerts_anomaly SET user_email = (
                     SELECT r.user_email FROM custom_anomaly_rules r
                     WHERE alerts_anomaly.anomaly_type = 'custom:' || r.id
                 )
                 WHERE anomaly_type LIKE 'custom:%' AND user_email IS NULL",
                [],
            )?;
            conn.execute(
                "INSERT INTO schema_migrations (id, applied_at) VALUES (?, ?)",
                params![ANOMALY_OWNER_MIGRATION, now()],
            )?;
            info!(recovered, "added alerts_anomaly.user_email");
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

        // ── Second factor encryption ──────────────────────────────
        //
        // Four states, and two of them refuse to start. A missing or wrong key
        // means every enrolled account is locked out either way; starting would
        // turn that into logins failing one at a time, and disable_2fa would
        // appear to work while login_2fa failed, quietly pushing users off their
        // second factor. Refusing is loud and is fixed by supplying the key.
        let jwt_secret = std::env::var("JWT_SECRET").unwrap_or_default();
        let secret_box = crate::secretbox::SecretBox::from_env(&jwt_secret)?;

        let totp_encryption_applied: i64 = conn.query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE id = ?",
            params![TOTP_ENCRYPTION_MIGRATION],
            |row| row.get(0),
        )?;
        if totp_encryption_applied == 0 {
            let plaintext: Vec<(String, String)> = {
                let mut stmt = conn.prepare(
                    "SELECT email, totp_secret FROM users WHERE totp_secret IS NOT NULL",
                )?;
                stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                    .collect::<Result<Vec<_>, _>>()?
            };
            if !plaintext.is_empty() {
                let Some(ref sb) = secret_box else {
                    return Err(DbError::EncryptionKeyMissing {
                        count: plaintext.len() as i64,
                    });
                };
                for (email, secret) in &plaintext {
                    let sealed = sb.seal(secret).map_err(|_| DbError::EncryptionUnavailable)?;
                    conn.execute(
                        "UPDATE users SET totp_secret_enc = ? WHERE email = ?",
                        params![sealed, email],
                    )?;
                }
            }
            conn.execute_batch("ALTER TABLE users DROP COLUMN IF EXISTS totp_secret")?;
            conn.execute(
                "INSERT INTO schema_migrations (id, applied_at) VALUES (?, ?)",
                params![TOTP_ENCRYPTION_MIGRATION, now()],
            )?;
            info!(
                migrated = plaintext.len(),
                "encrypted stored TOTP secrets and dropped the plaintext column"
            );
        }

        let rule_threshold_applied: i64 = conn.query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE id = ?",
            params![RULE_THRESHOLD_MIGRATION],
            |row| row.get(0),
        )?;
        if rule_threshold_applied == 0 {
            let existing: Vec<(String, String, f64)> = {
                let mut stmt = conn.prepare(
                    "SELECT id, metric, threshold FROM custom_anomaly_rules
                     WHERE threshold IS NOT NULL",
                )?;
                stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
                    .collect::<Result<Vec<_>, _>>()?
            };
            for (id, metric, threshold) in &existing {
                // A stored threshold was already accepted once, so it is not
                // rejected here. Anything the scale cannot hold is recorded and
                // the nearest representable value kept, since dropping the rule
                // silently would be worse.
                let scaled = crate::anomaly::scale_threshold(metric, *threshold)
                    .unwrap_or_else(|e| {
                        let m = crate::anomaly::metric_scale(metric);
                        let fallback = m.map_or(*threshold, |m| (*threshold * m.scale).round());
                        warn!(
                            rule = id.as_str(),
                            metric = metric.as_str(),
                            threshold = *threshold,
                            "threshold does not fit the metric scale ({e:?}), keeping nearest"
                        );
                        fallback as i64
                    });
                conn.execute(
                    "UPDATE custom_anomaly_rules SET threshold_scaled = ? WHERE id = ?",
                    params![scaled, id],
                )?;
            }
            conn.execute_batch(
                "ALTER TABLE custom_anomaly_rules DROP COLUMN IF EXISTS threshold",
            )?;
            conn.execute(
                "INSERT INTO schema_migrations (id, applied_at) VALUES (?, ?)",
                params![RULE_THRESHOLD_MIGRATION, now()],
            )?;
            info!(
                migrated = existing.len(),
                "scaled custom rule thresholds and dropped the floating point column"
            );
        }

        // usage_records held one row per user, so each new period overwrote the
        // last and no history survived. DuckDB cannot change a primary key in
        // place, so this is a rebuild: create, copy, drop, rename. Payment is
        // not connected yet, which is exactly why this is cheap now.
        let usage_history_applied: i64 = conn.query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE id = ?",
            params![USAGE_HISTORY_MIGRATION],
            |row| row.get(0),
        )?;
        if usage_history_applied == 0 {
            let carried: i64 = conn
                .query_row("SELECT COUNT(*) FROM usage_records", [], |row| row.get(0))
                .unwrap_or(0);
            conn.execute_batch(
                "CREATE TABLE usage_records_new (
                     user_email    TEXT   NOT NULL,
                     request_count BIGINT NOT NULL DEFAULT 0,
                     period_start  BIGINT NOT NULL,
                     period_end    BIGINT NOT NULL,
                     updated_at    BIGINT NOT NULL,
                     PRIMARY KEY (user_email, period_start)
                 );
                 INSERT INTO usage_records_new
                     SELECT user_email, request_count, period_start, period_end, updated_at
                     FROM usage_records;
                 DROP TABLE usage_records;
                 ALTER TABLE usage_records_new RENAME TO usage_records;",
            )?;
            conn.execute(
                "INSERT INTO schema_migrations (id, applied_at) VALUES (?, ?)",
                params![USAGE_HISTORY_MIGRATION, now()],
            )?;
            info!(carried, "rebuilt usage_records keyed by period");
        }

        // xray was keyed on (time_tag, energy), so when NOAA switches the primary
        // satellite and republishes a minute under a new number, the second
        // reading collided with the first and ON CONFLICT DO NOTHING discarded
        // it. Which satellite won each row therefore depended on arrival order,
        // which is why the satellite column cannot be trusted for any row
        // written before this migration. Existing rows are kept: the flux series
        // is what anything actually reads, and a hole in it would be worse than
        // an unreliable label on a column nothing renders.
        let xray_key_applied: i64 = conn.query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE id = ?",
            params![XRAY_SATELLITE_KEY_MIGRATION],
            |row| row.get(0),
        )?;
        if xray_key_applied == 0 {
            let before: i64 = conn
                .query_row("SELECT COUNT(*) FROM xray", [], |row| row.get(0))
                .unwrap_or(0);
            let started = std::time::Instant::now();
            conn.execute_batch(
                "CREATE TABLE xray_new (
                     time_tag          TEXT    NOT NULL,
                     energy            TEXT    NOT NULL,
                     satellite         INTEGER NOT NULL,
                     flux_e12          BIGINT  NOT NULL,
                     observed_flux_e12 BIGINT  NOT NULL,
                     observed_at       BIGINT,
                     fetched_at        BIGINT  NOT NULL,
                     PRIMARY KEY (time_tag, energy, satellite)
                 );
                 INSERT INTO xray_new
                     SELECT time_tag, energy, satellite, flux_e12, observed_flux_e12,
                            observed_at, fetched_at
                     FROM xray;
                 DROP TABLE xray;
                 ALTER TABLE xray_new RENAME TO xray;",
            )?;
            let after: i64 = conn.query_row("SELECT COUNT(*) FROM xray", [], |row| row.get(0))?;
            if after != before {
                error!(
                    before,
                    after, "xray rebuild did not preserve every row"
                );
            }
            conn.execute(
                "INSERT INTO schema_migrations (id, applied_at) VALUES (?, ?)",
                params![XRAY_SATELLITE_KEY_MIGRATION, now()],
            )?;
            info!(
                rows = after,
                ms = started.elapsed().as_millis() as u64,
                "rebuilt xray with satellite in the primary key"
            );
        }

        let retire_starter_applied: i64 = conn.query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE id = ?",
            params![RETIRE_STARTER_MIGRATION],
            |row| row.get(0),
        )?;
        if retire_starter_applied == 0 {
            let moved = conn.execute("UPDATE users SET plan = 'free' WHERE plan = 'starter'", [])?;
            conn.execute_batch("ALTER TABLE users ALTER COLUMN plan SET DEFAULT 'free'")?;
            conn.execute(
                "INSERT INTO schema_migrations (id, applied_at) VALUES (?, ?)",
                params![RETIRE_STARTER_MIGRATION, now()],
            )?;
            info!(moved, "retired the starter tier, accounts moved to free");
        }

        let enrolled: i64 = conn.query_row(
            "SELECT COUNT(*) FROM users WHERE totp_secret_enc IS NOT NULL",
            [],
            |row| row.get(0),
        )?;
        match (&secret_box, enrolled) {
            (None, 0) => warn!(
                "TOTP_ENCRYPTION_KEY is not set. No account has a second factor, so this is \
                 allowed, but enabling 2FA will be refused until a key is configured."
            ),
            (None, count) => return Err(DbError::EncryptionKeyMissing { count }),
            (Some(sb), count) if count > 0 => {
                // A wrong key is as damaging as a missing one and far more
                // likely, through a copy paste error or a restore from the wrong
                // environment. It is only detectable by trying.
                let sample: String = conn.query_row(
                    "SELECT totp_secret_enc FROM users WHERE totp_secret_enc IS NOT NULL LIMIT 1",
                    [],
                    |row| row.get(0),
                )?;
                if sb.open(&sample).is_err() {
                    return Err(DbError::EncryptionKeyWrong);
                }
                info!(enrolled = count, "second factor encryption key verified");
            }
            (Some(_), _) => {}
        }

        Ok(Self { conn, secret_box })
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
            secret_box: crate::secretbox::SecretBox::from_env(
                &std::env::var("JWT_SECRET").unwrap_or_default(),
            )?,
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
             ON CONFLICT (date) DO UPDATE SET fetched_at = excluded.fetched_at",
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
        // Optimisation only: skip the mutex and the empty transaction. This
        // table is append only, so falling through would write no rows and
        // lose nothing. Contrast insert_starlink_batch, where the same check
        // is the only thing preventing a full replace from wiping the table.
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
                 ON CONFLICT (identifier) DO UPDATE SET fetched_at = excluded.fetched_at",
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
                 ON CONFLICT (id, close_approach_date) DO UPDATE SET fetched_at = excluded.fetched_at",
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
        // Optimisation only: skip the mutex and the empty transaction. This
        // table is append only, so falling through would write no rows and
        // lose nothing. Contrast insert_starlink_batch, where the same check
        // is the only thing preventing a full replace from wiping the table.
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
                 ON CONFLICT (pl_name) DO UPDATE SET fetched_at = excluded.fetched_at",
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
        // Optimisation only: skip the mutex and the empty transaction. This
        // table is append only, so falling through would write no rows and
        // lose nothing. Contrast insert_starlink_batch, where the same check
        // is the only thing preventing a full replace from wiping the table.
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
        // Optimisation only: skip the mutex and the empty transaction. This
        // table is append only, so falling through would write no rows and
        // lose nothing. Contrast insert_starlink_batch, where the same check
        // is the only thing preventing a full replace from wiping the table.
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
        // Optimisation only: skip the mutex and the empty transaction. This
        // table is append only, so falling through would write no rows and
        // lose nothing. Contrast insert_starlink_batch, where the same check
        // is the only thing preventing a full replace from wiping the table.
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
            // The poller re-reads the whole one day window every two minutes, so
            // the same reading is offered many times. NOAA applies corrections,
            // which the payload advertises with electron_correction and
            // electron_contaminaton, so first-write-wins would pin the original
            // and discard every later revision. kp and kp_3h already update for
            // the same reason.
            //
            // The measured values update: flux_e12 and observed_flux_e12, plus
            // fetched_at to record when the newest version arrived. observed_at
            // does not, because it is derived from time_tag and so cannot change
            // without the key changing. time_tag, energy and satellite are the
            // key itself: they identify which reading this is, so a different
            // value there is a different row, not a revision of this one.
            let mut stmt = self.conn.prepare(&format!(
                "INSERT INTO xray
                 (time_tag, energy, satellite, flux_e12, observed_flux_e12, observed_at, fetched_at)
                 VALUES (?, ?, ?, ?, ?, {OBSERVED_AT_PARAM_SQL}, ?)
                 ON CONFLICT (time_tag, energy, satellite) DO UPDATE SET
                   flux_e12          = excluded.flux_e12,
                   observed_flux_e12 = excluded.observed_flux_e12,
                   fetched_at        = excluded.fetched_at"
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
        // Optimisation only: skip the mutex and the empty transaction. This
        // table is append only, so falling through would write no rows and
        // lose nothing. Contrast insert_starlink_batch, where the same check
        // is the only thing preventing a full replace from wiping the table.
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
        // Optimisation only: skip the mutex and the empty transaction. This
        // table is append only, so falling through would write no rows and
        // lose nothing. Contrast insert_starlink_batch, where the same check
        // is the only thing preventing a full replace from wiping the table.
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

    /// Writes the whole returned window rather than only hours newer than the
    /// newest stored one.
    ///
    /// Kyoto publishes Dst provisionally and corrects it afterwards, and the
    /// feed re-offers a rolling window of past hours with values that can have
    /// changed. Two separate mechanisms were discarding those corrections: the
    /// incremental filter dropped every hour at or below the newest stored one
    /// before the insert ran, and the conflict clause would have dropped any
    /// that survived. The stored series was therefore provisional values frozen
    /// at first sight, permanently.
    ///
    /// Both are removed here. The cost is upserting the returned window each
    /// poll instead of only its new rows, which for this feed is 168 rows every
    /// 300 seconds. `kp` and `kp_3h` already work this way.
    pub fn insert_dst_batch(&self, records: &[DstRecord]) -> Result<(), DbError> {
        // Optimisation only: skip the mutex and the empty transaction. This
        // table is append only, so falling through would write no rows and
        // lose nothing. Contrast insert_starlink_batch, where the same check
        // is the only thing preventing a full replace from wiping the table.
        if records.is_empty() {
            return Ok(());
        }
        self.begin()?;
        let result = (|| {
            let mut stmt = self.conn.prepare(&format!(
                "INSERT INTO dst (time_tag, dst_nt, observed_at, fetched_at)
                 VALUES (?, ?, {OBSERVED_AT_PARAM_SQL}, ?)
                 ON CONFLICT (time_tag) DO UPDATE SET
                   dst_nt     = excluded.dst_nt,
                   fetched_at = excluded.fetched_at"
            ))?;
            for r in records {
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
        // Optimisation only: skip the mutex and the empty transaction. This
        // table is append only, so falling through would write no rows and
        // lose nothing. Contrast insert_starlink_batch, where the same check
        // is the only thing preventing a full replace from wiping the table.
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
    ///
    /// Stale input fails on the same footing. A full window of readings that
    /// stopped arriving weeks ago still yields a forecast that reads as current,
    /// which is the same wrong answer by a different route. The limit is the one
    /// SERIES_FRESHNESS already holds for this series, so the model, the chart
    /// and the status page all agree on when kp_3h has gone quiet.
    pub fn get_recent_kp_3h(&self, n: usize) -> Result<Vec<f64>, DbError> {
        if !self.series_is_current("kp_3h")? {
            return Err(DbError::StaleSeries {
                series: "kp_3h",
                newest_observed_at: self.newest_observation("kp_3h")?,
            });
        }
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

    /// Newest observation time in a series, or None when nothing is stored.
    ///
    /// `table` always comes from SERIES_FRESHNESS, never from a request.
    fn newest_observation(&self, table: &str) -> Result<Option<i64>, DbError> {
        let Some(series) = SERIES_FRESHNESS.iter().find(|s| s.table == table) else {
            return Ok(None);
        };
        // Both identifiers come from SERIES_FRESHNESS, never from a request.
        let sql = format!("SELECT MAX({}) FROM {table}", series.time_column);
        Ok(self.conn.query_row(&sql, [], |row| row.get(0))?)
    }

    /// True when a series is recent enough to draw.
    ///
    /// An empty table and an unrecognised name both read as not current, so a
    /// caller never draws on a guess.
    fn series_is_current(&self, table: &str) -> Result<bool, DbError> {
        let Some(limit) = SERIES_FRESHNESS.iter().find(|s| s.table == table) else {
            return Ok(false);
        };
        match self.newest_observation(table)? {
            Some(newest) => Ok(now() - newest <= limit.max_age_secs),
            None => Ok(false),
        }
    }

    /// Per-series status for the status page: component, status, and the newest
    /// observation time. Read from `observed_at`, the time the measurement was
    /// taken, not from `fetched_at`, which keeps moving whenever a poll runs
    /// even if the poll brought nothing new.
    pub fn series_health(&self) -> Vec<(&'static str, &'static str, Option<i64>)> {
        let now = now();
        SERIES_FRESHNESS
            .iter()
            .map(|s| {
                let newest = self.newest_observation(s.table).ok().flatten();
                let status = match newest {
                    None => "unknown",
                    Some(t) if now - t > s.max_age_secs => "degraded",
                    Some(_) => "operational",
                };
                (s.component, status, newest)
            })
            .collect()
    }

    /// The newest recorded verdict for each `POLL_LIVENESS` component.
    ///
    /// Mirrors `series_health` in shape so the health handler can treat the two
    /// the same way. A component with no verdict at all is `unknown`, one whose
    /// newest verdict is older than its limit is `degraded`, because a poller
    /// that has stopped writing is a fault and not an absence.
    pub fn poll_liveness(&self) -> Vec<(&'static str, &'static str, Option<i64>)> {
        let now = now();
        POLL_LIVENESS
            .iter()
            .map(|l| {
                let newest = self.newest_poll_verdict(l.component).ok().flatten();
                let status = match newest {
                    None => "unknown",
                    Some((_, ts)) if now - ts > l.max_verdict_age_secs => "degraded",
                    Some((status, _)) => status,
                };
                (l.component, status, newest.map(|(_, ts)| ts))
            })
            .collect()
    }

    /// The newest `health_snapshots` row for one component, as `(status, ts)`.
    ///
    /// `component` always comes from `POLL_LIVENESS`, never from a request. The
    /// status is mapped back onto the static strings the rest of the health path
    /// uses, so an unrecognised value stored by an older build reads as
    /// `unknown` rather than reaching a caller as an arbitrary string.
    fn newest_poll_verdict(
        &self,
        component: &str,
    ) -> Result<Option<(&'static str, i64)>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT status, ts FROM health_snapshots
             WHERE component = ? ORDER BY ts DESC LIMIT 1",
        )?;
        let mut rows = stmt.query(params![component])?;
        match rows.next()? {
            None => Ok(None),
            Some(row) => {
                let status: String = row.get(0)?;
                let ts: i64 = row.get(1)?;
                let status = match status.as_str() {
                    "operational" => "operational",
                    "degraded" => "degraded",
                    _ => "unknown",
                };
                Ok(Some((status, ts)))
            }
        }
    }

    /// Most recent 1440 Kp readings, oldest-first. Selected DESC so the LIMIT
    /// takes the newest window, then reversed for the caller.
    ///
    /// A series past its freshness limit returns empty rather than its last
    /// good window, so the panel draws nothing instead of drawing old readings
    /// as current.
    pub fn get_kp_recent(&self) -> Result<serde_json::Value, DbError> {
        if !self.series_is_current("kp")? {
            return Ok(serde_json::Value::Array(Vec::new()));
        }
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

    /// Most recent 240 three-hour Kp readings, oldest-first. Empty when stale.
    pub fn get_kp_3h_recent(&self) -> Result<serde_json::Value, DbError> {
        if !self.series_is_current("kp_3h")? {
            return Ok(serde_json::Value::Array(Vec::new()));
        }
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

    /// Most recent 1440 solar wind readings, oldest-first. Empty when stale.
    pub fn get_solar_wind_recent(&self) -> Result<serde_json::Value, DbError> {
        if !self.series_is_current("solar_wind")? {
            return Ok(serde_json::Value::Array(Vec::new()));
        }
        let mut stmt = self.conn.prepare(
            "SELECT time_tag, speed_e1, density_e2, temp_k FROM solar_wind \
             ORDER BY observed_at DESC LIMIT 1440",
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
    /// because each carries both the long and short energy band. Empty when stale.
    pub fn get_xray_recent(&self) -> Result<serde_json::Value, DbError> {
        if !self.series_is_current("xray")? {
            return Ok(serde_json::Value::Array(Vec::new()));
        }
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

    /// Most recent 1440 IMF readings, oldest-first. Empty when stale.
    pub fn get_imf_recent(&self) -> Result<serde_json::Value, DbError> {
        if !self.series_is_current("imf")? {
            return Ok(serde_json::Value::Array(Vec::new()));
        }
        let mut stmt = self
            .conn
            .prepare("SELECT time_tag, bz_e2, bt_e2 FROM imf ORDER BY observed_at DESC LIMIT 1440")?;
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

    /// Most recent 1440 Dst readings, oldest-first. Empty when stale.
    pub fn get_dst_recent(&self) -> Result<serde_json::Value, DbError> {
        if !self.series_is_current("dst")? {
            return Ok(serde_json::Value::Array(Vec::new()));
        }
        let mut stmt = self
            .conn
            .prepare("SELECT time_tag, dst_nt FROM dst ORDER BY observed_at DESC LIMIT 1440")?;
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

    /// Latest ISS position, or null when the feed has stopped. A stale fix is
    /// worse than none: it draws the station parked at one point with nothing
    /// indicating the position is old.
    pub fn get_iss_latest(&self) -> Result<serde_json::Value, DbError> {
        if !self.series_is_current("iss_position")? {
            return Ok(serde_json::Value::Null);
        }
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
    /// Encrypted, in the `v1:{nonce}:{ciphertext}` form. Use `totp_secret` to
    /// read the plaintext; the raw value is never usable directly.
    pub totp_secret_enc: Option<String>,
    pub totp_enabled: bool,
}

impl Store {
    pub fn create_user(&self, email: &str, hash: &str) -> Result<(), DbError> {
        let result = self.conn.execute(
            "INSERT INTO users (email, password_hash, created_at, plan) VALUES (?, ?, ?, 'free')",
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
             VALUES (?, ?, ?, 'free', TRUE, ?)",
            params![email, hash, now(), provider],
        );
        match result {
            Ok(_) => Ok(()),
            Err(e) if e.to_string().contains("Constraint Error") => Err(DbError::EmailTaken),
            Err(e) => Err(DbError::Duckdb(e)),
        }
    }

    /// Sets a new password and invalidates every session issued before it, in
    /// one statement so the two cannot be applied separately.
    pub fn update_password_hash(&self, email: &str, new_hash: &str) -> Result<(), DbError> {
        self.conn.execute(
            "UPDATE users SET password_hash = ?, token_version = COALESCE(token_version, 0) + 1              WHERE email = ?",
            params![new_hash, email],
        )?;
        Ok(())
    }

    /// Current token version for an account. A missing account reads as 0, which
    /// no valid token can match because the extractor also checks the account
    /// exists via its plan lookup.
    pub fn get_token_version(&self, email: &str) -> Result<i64, DbError> {
        Ok(self
            .conn
            .query_row(
                "SELECT COALESCE(token_version, 0) FROM users WHERE email = ?",
                params![email],
                |row| row.get(0),
            )
            .unwrap_or(0))
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
            "SELECT email, password_hash, email_verified, totp_secret_enc, totp_enabled \
             FROM users WHERE email = ?",
        )?;
        let mut rows = stmt.query([email])?;
        if let Some(row) = rows.next()? {
            Ok(Some(User {
                email: row.get(0)?,
                password_hash: row.get(1)?,
                email_verified: row.get::<_, Option<bool>>(2)?.unwrap_or(false),
                totp_secret_enc: row.get(3)?,
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
                    .unwrap_or_else(|| "free".to_string());
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
                "plan":           "free",
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

/// Which `alerts_anomaly` rows an account may see: every global detection, plus
/// the ones its own rules raised. Binds one parameter, the caller's email.
///
/// It is a shared constant rather than a clause written out per query because
/// AUD-008 was fixed once and stayed open: `get_anomalies_recent` was scoped and
/// `get_events_page`, which the finding named in the same list, was not, because
/// the rule lived inside one query and nothing carried it to the others.
/// `get_anomalies_recent` still spells the same rule as two separately windowed
/// selects, for the starvation reason given on it, so it does not read this
/// constant. `every_anomaly_read_path_is_scoped_to_the_caller` is what holds all
/// three read paths to the one rule.
const ANOMALY_VISIBLE_TO: &str = "(user_email IS NULL OR user_email = ?)";

impl Store {
    /// `user_email` is None for a global detection and Some for an anomaly
    /// raised by one account's custom rule.
    pub fn insert_anomaly(
        &self,
        anomaly_type: &str,
        source_ref: &str,
        severity: &str,
        message: &str,
        user_email: Option<&str>,
    ) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO alerts_anomaly
                 (anomaly_type, source_ref, detected_at, severity, message, user_email)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT (anomaly_type, source_ref) DO NOTHING",
            params![anomaly_type, source_ref, now(), severity, message, user_email],
        )?;
        Ok(())
    }

    /// Paginated, filtered browse of past anomaly events.
    /// `since` is a unix-seconds cutoff. `type_filter` and `severity_filter`
    /// are optional exact matches. Returns rows + total count for pagination.
    pub fn get_events_page(
        &self,
        user_email: &str,
        since: i64,
        type_filter: Option<&str>,
        severity_filter: Option<&str>,
        page: i64,
        page_size: i64,
    ) -> Result<serde_json::Value, DbError> {
        let mut where_clauses = vec!["detected_at > ?".to_string()];
        let mut bindings: Vec<duckdb::types::Value> = vec![since.into()];
        where_clauses.push(ANOMALY_VISIBLE_TO.to_string());
        bindings.push(user_email.to_string().into());
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

    /// Anomalies visible to one account: every global detection, plus the ones
    /// raised by that account's own custom rules.
    ///
    /// Implements [`ANOMALY_VISIBLE_TO`] in a different shape, not a different
    /// rule. `every_anomaly_read_path_is_scoped_to_the_caller` asserts they
    /// agree.
    ///
    /// The two are windowed separately on purpose. A single ORDER BY over the
    /// union let one noisy rule fill the whole limit and push real global
    /// anomalies out of the feed, and before `user_email` existed it also served
    /// every account's rule names and thresholds to every authenticated caller.
    pub fn get_anomalies_recent(&self, user_email: &str) -> Result<serde_json::Value, DbError> {
        const GLOBAL_LIMIT: i64 = 100;
        const OWN_LIMIT: i64 = 50;
        let mut stmt = self.conn.prepare(
            "SELECT anomaly_type, source_ref, detected_at, severity, message, source FROM (
                 SELECT *, 'global' AS source FROM alerts_anomaly WHERE user_email IS NULL
                 ORDER BY detected_at DESC LIMIT ?
             )
             UNION ALL
             SELECT anomaly_type, source_ref, detected_at, severity, message, source FROM (
                 SELECT *, 'rule' AS source FROM alerts_anomaly WHERE user_email = ?
                 ORDER BY detected_at DESC LIMIT ?
             )
             ORDER BY detected_at DESC",
        )?;
        let rows = stmt
            .query_map(params![GLOBAL_LIMIT, user_email, OWN_LIMIT], |row| {
                let anomaly_type: String = row.get(0)?;
                let source_ref: String = row.get(1)?;
                let detected_at: i64 = row.get(2)?;
                let severity: String = row.get(3)?;
                let message: String = row.get(4)?;
                // "global" is a detection that applies to everyone; "rule" is
                // one the caller's own custom rule raised. Stated by the server
                // so a reader does not have to infer it from the type string.
                let source: String = row.get(5)?;
                Ok(serde_json::json!({
                    "type":        anomaly_type,
                    "source_ref":  source_ref,
                    "detected_at": detected_at,
                    "severity":    severity,
                    "message":     message,
                    "source":      source,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(serde_json::Value::Array(rows))
    }

    // ── Raw queries for anomaly detection ─────────────────────────────────────

    /// Newest Kp reading as `(time_tag, observed_at, value)`.
    ///
    /// The observation time comes back with it because a caller deciding
    /// whether to act on this needs to know how old it is. The email alert
    /// dispatcher had the tuple and discarded the age, so it alerted from a
    /// reading of any age (AUD-028).
    pub fn latest_kp_raw(&self) -> Result<Option<(String, i64, i64)>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT time_tag, observed_at, estimated_kp_e2 FROM kp ORDER BY observed_at DESC LIMIT 1",
        )?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            Ok(Some((row.get(0)?, row.get(1)?, row.get(2)?)))
        } else {
            Ok(None)
        }
    }

    /// Newest solar wind speed as `(time_tag, observed_at, value)`, for the same
    /// reason as [`Store::latest_kp_raw`].
    pub fn latest_solar_wind_speed_raw(&self) -> Result<Option<(String, i64, i64)>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT time_tag, observed_at, speed_e1 FROM solar_wind WHERE speed_e1 IS NOT NULL ORDER BY observed_at DESC LIMIT 1",
        )?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            Ok(Some((row.get(0)?, row.get(1)?, row.get(2)?)))
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
    /// Close approaches inside a forward window, by the date of the approach.
    ///
    /// The window is the approach, not the ingest. This filtered on `fetched_at`
    /// against a backward window, which is the wrong idea twice over for
    /// forward dated data: the poller refetches every thirty minutes so every
    /// stored row is always recent and the filter excluded nothing, and an
    /// asteroid that passed last week is not a warning however recently we
    /// heard about it (AUD-024).
    ///
    /// `close_approach_date` is a `YYYY-MM-DD` string, so it compares
    /// lexicographically and the bounds are dates rather than instants. The
    /// lower bound is today rather than now, because an approach later today
    /// still counts.
    pub fn neo_close_approaches_raw(
        &self,
        max_dist_scaled: i64,
        horizon_days: i64,
    ) -> Result<Vec<(String, String, i64)>, DbError> {
        let today = chrono::DateTime::from_timestamp(now(), 0)
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_default();
        let horizon = chrono::DateTime::from_timestamp(now() + horizon_days * 86_400, 0)
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_default();
        let mut stmt = self.conn.prepare(
            "SELECT id, close_approach_date, miss_distance_m FROM neo \
             WHERE miss_distance_m < ? AND close_approach_date >= ? AND close_approach_date <= ? \
             ORDER BY miss_distance_m ASC",
        )?;
        let rows = stmt
            .query_map(params![max_dist_scaled, today, horizon], |row| {
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
    /// In the metric's stored units, not the caller's. Use
    /// `anomaly::unscale_threshold` to display it.
    pub threshold_scaled: i64,
    pub severity: String,
    pub enabled: bool,
    pub created_at: i64,
}

impl Store {
    pub fn insert_custom_rule(&self, rule: &CustomRule) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO custom_anomaly_rules
             (id, user_email, name, metric, operator, threshold_scaled, severity, enabled, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                rule.id,
                rule.user_email,
                rule.name,
                rule.metric,
                rule.operator,
                rule.threshold_scaled,
                rule.severity,
                rule.enabled,
                rule.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn list_custom_rules(&self, user_email: &str) -> Result<Vec<CustomRule>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, user_email, name, metric, operator, threshold_scaled, severity, enabled, created_at
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
                    threshold_scaled: row.get(5)?,
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
            "SELECT id, user_email, name, metric, operator, threshold_scaled, severity, enabled, created_at
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
                    threshold_scaled: row.get(5)?,
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
    pub fn get_report_summary(
        &self,
        user_email: &str,
        since_secs: i64,
    ) -> Result<serde_json::Value, DbError> {
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

        // Anomaly count in window. Scoped, because an unscoped count still
        // reports how often other accounts' rules fired, and the figure is
        // labelled as this account's anomalies on the Reports card.
        let anomaly_count: i64 = {
            let sql = format!(
                "SELECT COUNT(*) FROM alerts_anomaly WHERE detected_at > ? AND {ANOMALY_VISIBLE_TO}"
            );
            let mut stmt = self.conn.prepare(&sql)?;
            let mut rows = stmt.query(params![cutoff, user_email])?;
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
    /// Returns the last 60 Kp readings oldest-first - same shape as /api/kp,
    /// and empty on the same terms when the series is stale.
    pub fn get_kp_array_public(&self) -> Result<serde_json::Value, DbError> {
        if !self.series_is_current("kp")? {
            return Ok(serde_json::Value::Array(Vec::new()));
        }
        let mut stmt = self.conn.prepare(
            "SELECT time_tag, kp_index, estimated_kp_e2 FROM kp ORDER BY observed_at DESC LIMIT 60",
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
    pub expires_at: Option<i64>,
    pub revoked_at: Option<i64>,
}

impl Store {
    pub fn create_api_key(
        &self,
        id: &str,
        user_email: &str,
        key_hash: &str,
        name: &str,
        expires_at: Option<i64>,
    ) -> Result<(), DbError> {
        let result = self.conn.execute(
            "INSERT INTO api_keys
                 (id, user_email, key_hash, name, created_at, request_count, expires_at)
             VALUES (?, ?, ?, ?, ?, 0, ?)",
            params![id, user_email, key_hash, name, now(), expires_at],
        );
        match result {
            Ok(_) => Ok(()),
            Err(e) if e.to_string().contains("Constraint Error") => Err(DbError::KeyNotFound),
            Err(e) => Err(DbError::Duckdb(e)),
        }
    }

    pub fn list_api_keys(&self, user_email: &str) -> Result<Vec<ApiKey>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, created_at, last_used_at, request_count, expires_at, revoked_at \
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
                    expires_at: row.get(5)?,
                    revoked_at: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Returns true if deleted, false if key not found for this user.
    /// Revokes a key rather than deleting it, so its name and request count
    /// survive as an audit trail. The lookup refuses revoked keys, so the effect
    /// on a caller is the same as deletion was.
    pub fn revoke_api_key(&self, id: &str, user_email: &str) -> Result<bool, DbError> {
        let n = self.conn.execute(
            "UPDATE api_keys SET revoked_at = ?
             WHERE id = ? AND user_email = ? AND revoked_at IS NULL",
            params![now(), id, user_email],
        )?;
        Ok(n > 0)
    }

    /// Keys an account can still use. Revoked and expired keys do not count, so
    /// revoking one frees a slot under the cap.
    pub fn count_active_api_keys(&self, user_email: &str) -> Result<i64, DbError> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM api_keys
             WHERE user_email = ? AND revoked_at IS NULL
               AND (expires_at IS NULL OR expires_at > ?)",
            params![user_email, now()],
            |row| row.get(0),
        )?)
    }

    /// Returns the user_email for the given key hash, if it exists.
    pub fn find_api_key_by_hash(&self, key_hash: &str) -> Result<Option<String>, DbError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT user_email FROM api_keys
                 WHERE key_hash = ? AND revoked_at IS NULL
                   AND (expires_at IS NULL OR expires_at > ?)
                 LIMIT 1",
            )?;
        let mut rows = stmt.query(params![key_hash, now()])?;
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

    /// Decrypts a stored second factor. None when the account has none.
    pub fn totp_secret(&self, user: &User) -> Result<Option<String>, DbError> {
        let Some(ref sealed) = user.totp_secret_enc else {
            return Ok(None);
        };
        let Some(ref sb) = self.secret_box else {
            return Err(DbError::EncryptionUnavailable);
        };
        sb.open(sealed)
            .map(Some)
            .map_err(|_| DbError::EncryptionKeyWrong)
    }

    /// Stores an encrypted second factor. Fails when no key is configured, so
    /// a secret is never written in the clear.
    pub fn set_totp_secret(&self, email: &str, secret: &str) -> Result<(), DbError> {
        let Some(ref sb) = self.secret_box else {
            return Err(DbError::EncryptionUnavailable);
        };
        let sealed = sb.seal(secret).map_err(|_| DbError::EncryptionUnavailable)?;
        self.conn.execute(
            "UPDATE users SET totp_secret_enc = ? WHERE email = ?",
            params![sealed, email],
        )?;
        Ok(())
    }

    /// Turning the second factor on or off is a change to how the account
    /// authenticates, so it invalidates tokens minted before it, the same way a
    /// password change does.
    ///
    /// Without the bump, somebody who turns 2FA on because they believe their
    /// session is stolen leaves the thief holding a working token for up to
    /// twenty four hours: the countermeasure does not touch the thing it was
    /// taken against. `every_factor_change_invalidates_sessions` holds all three
    /// writers to this.
    pub fn enable_totp(&self, email: &str) -> Result<(), DbError> {
        self.conn.execute(
            "UPDATE users SET totp_enabled = TRUE, token_version = COALESCE(token_version, 0) + 1 WHERE email = ?",
            params![email],
        )?;
        Ok(())
    }

    /// Disabling matters as much as enabling: a thief who turns 2FA off has
    /// weakened the account, and the owner turning it off wants a clean slate.
    pub fn disable_totp(&self, email: &str) -> Result<(), DbError> {
        self.conn.execute(
            "UPDATE users SET totp_secret_enc = NULL, totp_enabled = FALSE, token_version = COALESCE(token_version, 0) + 1 WHERE email = ?",
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
                .unwrap_or_else(|| "free".to_string()),
            None => "free".to_string(),
        })
    }

    /// Records usage for one account in one period.
    ///
    /// A zero count never creates a row. A dashboard only account is known to
    /// the counter, because the session path caches its token version there, but
    /// it spends no quota; writing it an empty row every period would accumulate
    /// one per account per period forever. The absence of a row is the same
    /// information.
    ///
    /// A row that already exists is still updated when the count is zero, so a
    /// correction downward is possible and nothing already recorded disappears.
    pub fn upsert_usage_record(
        &self,
        email: &str,
        count: i64,
        period_start: i64,
        period_end: i64,
    ) -> Result<(), DbError> {
        if count == 0 {
            let updated = self.conn.execute(
                "UPDATE usage_records SET request_count = ?, period_end = ?, updated_at = ?
                 WHERE user_email = ? AND period_start = ?",
                params![count, period_end, now(), email, period_start],
            )?;
            if updated == 0 {
                return Ok(());
            }
            return Ok(());
        }
        self.conn.execute(
            "INSERT INTO usage_records
                 (user_email, request_count, period_start, period_end, updated_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT (user_email, period_start) DO UPDATE SET
                 request_count = excluded.request_count,
                 period_end    = excluded.period_end,
                 updated_at    = excluded.updated_at",
            params![email, count, period_start, period_end, now()],
        )?;
        Ok(())
    }

    /// Usage for one account in one period. `None` means no row, which is the
    /// same as no usage; callers report zero rather than failing.
    pub fn get_usage_for_period(
        &self,
        email: &str,
        period_start: i64,
    ) -> Result<Option<(i64, i64, i64)>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT request_count, period_start, period_end FROM usage_records
             WHERE user_email = ? AND period_start = ?",
        )?;
        let mut rows = stmt.query(params![email, period_start])?;
        match rows.next()? {
            Some(row) => Ok(Some((row.get(0)?, row.get(1)?, row.get(2)?))),
            None => Ok(None),
        }
    }

    /// Past periods for one account, newest first. The billing history that one
    /// row per user could not hold.
    pub fn list_usage_history(
        &self,
        email: &str,
        limit: i64,
    ) -> Result<Vec<(i64, i64, i64)>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT request_count, period_start, period_end FROM usage_records
             WHERE user_email = ? ORDER BY period_start DESC LIMIT ?",
        )?;
        let rows = stmt
            .query_map(params![email, limit], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
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

    /// Every stored webhook target, across all accounts, as `(id, url)`.
    ///
    /// For the startup scan only: it reports which stored rows the current
    /// delivery rules refuse, so tightening those rules is visible at boot
    /// rather than only as deliveries that quietly stop.
    pub fn list_webhook_targets(&self) -> Result<Vec<(String, String)>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, url FROM webhooks WHERE active = true")?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
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
        // Load bearing, not an optimisation. Unlike every other batch insert,
        // this one is a full replace, so the DELETE below runs before any row is
        // written. An empty batch that got past here would commit a table with
        // nothing in it.
        //
        // Empty batches are routine: Celestrak refreshes every two hours and we
        // poll hourly, so roughly every other poll returns "no change" and the
        // poller hands the writer an empty vector. The poller does not skip that
        // call. This is the only thing standing between a normal 403 and an
        // emptied table, and `empty_starlink_batch_must_not_wipe_the_table`
        // fails if it is removed.
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

    /// Returns (nasa_last_write, celestrak_last_write) as Unix timestamps.
    /// Each is None if the table has no rows yet.
    ///
    /// The NOAA series are not here. They report one component each through
    /// `series_health`, because a single query over one table let a dead feed
    /// hide behind a live one.
    pub fn external_freshness(&self) -> Option<i64> {
        // The NASA aggregate that used to live here is gone: apod, neo, epic and
        // exoplanets each report themselves through SERIES_FRESHNESS, because
        // one live feed was hiding two dead ones.
        self.conn
            .query_row("SELECT MAX(fetched_at) FROM starlink", [], |row| {
                row.get::<_, Option<i64>>(0)
            })
            .ok()
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    /// Mirrors ONE_LD_SCALED in anomaly.rs. One lunar distance, in the units
    /// miss_distance_m actually holds.
    const ONE_LD_SCALED_FOR_TEST: i64 = 384_400_000;
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
        // Timestamps are recent because the read path drops a stale series.
        let base = now() - 120;
        store
            .insert_kp_batch(&[
                KpRecord { time_tag: iso(base), kp_index: 2, estimated_kp: 2.33 },
                KpRecord { time_tag: iso(base + 60), kp_index: 3, estimated_kp: 3.67 },
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

    /// A revoked key stops authenticating but keeps its row, so the name and
    /// request count survive as an audit trail. Deleting the row destroyed both.
    #[test]
    fn a_revoked_key_stops_working_but_keeps_its_history() {
        let store = mem_store();
        store
            .create_api_key("k1", "user@example.com", "hash-1", "laptop", None)
            .expect("create");
        assert_eq!(
            store.find_api_key_by_hash("hash-1").expect("lookup"),
            Some("user@example.com".to_string())
        );

        assert!(store.revoke_api_key("k1", "user@example.com").expect("revoke"));
        assert_eq!(
            store.find_api_key_by_hash("hash-1").expect("lookup"),
            None,
            "a revoked key must not authenticate"
        );

        let keys = store.list_api_keys("user@example.com").expect("list");
        assert_eq!(keys.len(), 1, "the row must survive revocation");
        assert_eq!(keys[0].name, "laptop");
        assert!(keys[0].revoked_at.is_some());

        // Revoking twice reports nothing further to do.
        assert!(!store.revoke_api_key("k1", "user@example.com").expect("revoke again"));
    }

    /// One account must not be able to revoke another account's key.
    #[test]
    fn revocation_is_scoped_to_the_owner() {
        let store = mem_store();
        store
            .create_api_key("k1", "owner@example.com", "hash-1", "key", None)
            .expect("create");
        assert!(!store.revoke_api_key("k1", "attacker@example.com").expect("revoke"));
        assert_eq!(
            store.find_api_key_by_hash("hash-1").expect("lookup"),
            Some("owner@example.com".to_string()),
            "the key must still work for its owner"
        );
    }

    /// An expired key stops authenticating on its own, with no revocation.
    #[test]
    fn an_expired_key_stops_working() {
        let store = mem_store();
        let past = now() - 60;
        let future = now() + 3_600;
        store
            .create_api_key("expired", "user@example.com", "hash-old", "old", Some(past))
            .expect("create expired");
        store
            .create_api_key("live", "user@example.com", "hash-new", "new", Some(future))
            .expect("create live");

        assert_eq!(store.find_api_key_by_hash("hash-old").expect("lookup"), None);
        assert_eq!(
            store.find_api_key_by_hash("hash-new").expect("lookup"),
            Some("user@example.com".to_string())
        );
        // A key with no expiry keeps working, which is every key made before this.
        store
            .create_api_key("forever", "user@example.com", "hash-forever", "old style", None)
            .expect("create unexpiring");
        assert_eq!(
            store.find_api_key_by_hash("hash-forever").expect("lookup"),
            Some("user@example.com".to_string())
        );
    }

    /// The cap counts only keys that can still be used, so revoking or letting
    /// one expire frees a slot.
    #[test]
    fn the_active_key_count_ignores_revoked_and_expired_keys() {
        let store = mem_store();
        let email = "counter@example.com";
        for i in 0..3 {
            store
                .create_api_key(&format!("k{i}"), email, &format!("h{i}"), "key", None)
                .expect("create");
        }
        assert_eq!(store.count_active_api_keys(email).expect("count"), 3);

        store.revoke_api_key("k0", email).expect("revoke");
        assert_eq!(store.count_active_api_keys(email).expect("count"), 2);

        store
            .create_api_key("expired", email, "h-exp", "key", Some(now() - 1))
            .expect("create expired");
        assert_eq!(
            store.count_active_api_keys(email).expect("count"),
            2,
            "an expired key must not occupy a slot"
        );

        // Another account's keys are counted separately.
        store
            .create_api_key("other", "someone@example.com", "h-other", "key", None)
            .expect("create");
        assert_eq!(store.count_active_api_keys(email).expect("count"), 2);
    }

    /// A series past its freshness limit must read as empty rather than as its
    /// last good window. Before this, a feed dead since June still drew a full
    /// day of June readings on a chart labelled current.
    #[test]
    fn a_stale_series_reads_as_empty() {
        let store = mem_store();
        let stale = now() - 40 * 86_400;
        store
            .insert_imf_batch(&[ImfRecord {
                time_tag: iso(stale),
                bz_gsm: Some(1.01),
                bt: Some(14.71),
            }])
            .unwrap();

        let out = store.get_imf_recent().unwrap();
        assert_eq!(
            out.as_array().unwrap().len(),
            0,
            "a series frozen forty days ago must not be served"
        );

        // The same read returns data once a current reading arrives.
        store
            .insert_imf_batch(&[ImfRecord {
                time_tag: iso(now() - 60),
                bz_gsm: Some(-2.5),
                bt: Some(9.0),
            }])
            .unwrap();
        let out = store.get_imf_recent().unwrap();
        assert_eq!(out.as_array().unwrap().len(), 2);
    }

    /// An empty table is not current either, so nothing is drawn on a guess.
    #[test]
    fn an_empty_series_reads_as_empty() {
        let store = mem_store();
        assert_eq!(store.get_imf_recent().unwrap().as_array().unwrap().len(), 0);
        assert_eq!(store.get_dst_recent().unwrap().as_array().unwrap().len(), 0);
    }

    /// The ISS feed polls every five seconds, so a stale fix means it stopped.
    /// Drawing the last known position with nothing marking it old put the
    /// station parked at one point on the map.
    #[test]
    fn a_stale_iss_position_reads_as_empty() {
        let store = mem_store();
        let fix = |ts: i64| crate::iss::IssPosition {
            latitude: 15.4372,
            longitude: 37.6327,
            altitude: 420.0,
            velocity: 27_600.0,
            timestamp: ts,
        };

        assert!(store.get_iss_latest().expect("empty table").is_null());

        store.insert_iss_position(&fix(now() - 3_600)).expect("insert stale");
        assert!(
            store.get_iss_latest().expect("stale").is_null(),
            "an hour old fix must not be served as the current position"
        );

        store.insert_iss_position(&fix(now() - 5)).expect("insert current");
        let v = store.get_iss_latest().expect("current");
        assert!(!v.is_null(), "a current fix must be served");
        assert!((v["latitude"].as_f64().expect("lat") - 15.4372).abs() < 1e-5);
    }

    /// Freshness reads whichever column holds the observation time. The NOAA
    /// series derive `observed_at`; `iss_position` is already keyed on an epoch.
    #[test]
    fn every_series_names_the_column_holding_its_observation_time() {
        let store = mem_store();
        for series in SERIES_FRESHNESS {
            store
                .newest_observation(series.table)
                .unwrap_or_else(|e| panic!("{} on {}: {e}", series.time_column, series.table));
        }
        let iss = SERIES_FRESHNESS
            .iter()
            .find(|s| s.table == "iss_position")
            .expect("iss_position has a freshness entry");
        assert_eq!(iss.time_column, "ts");
        assert_eq!(iss.component, "iss");
    }

    /// One account's custom rule anomalies must not reach another account, and
    /// global detections must reach everyone. Before user_email existed, every
    /// authenticated caller read every account's rule names and thresholds.
    #[test]
    fn anomalies_are_scoped_to_global_plus_the_callers_own() {
        let store = mem_store();
        store
            .insert_anomaly("kp_storm", "g1", "warning", "Kp 5.0", None)
            .expect("global");
        store
            .insert_anomaly("custom:r1", "r1:1", "warning", "alice rule", Some("alice@example.com"))
            .expect("alice");
        store
            .insert_anomaly("custom:r2", "r2:1", "critical", "bob secret threshold", Some("bob@example.com"))
            .expect("bob");

        let msgs = |email: &str| -> Vec<String> {
            store
                .get_anomalies_recent(email)
                .expect("read")
                .as_array()
                .expect("array")
                .iter()
                .map(|v| v["message"].as_str().unwrap_or_default().to_string())
                .collect()
        };

        // Each row says which kind it is, so the reader is not left inferring it.
        let sources: Vec<(String, String)> = store
            .get_anomalies_recent("alice@example.com")
            .expect("read")
            .as_array()
            .expect("array")
            .iter()
            .map(|v| {
                (
                    v["message"].as_str().unwrap_or_default().to_string(),
                    v["source"].as_str().unwrap_or_default().to_string(),
                )
            })
            .collect();
        for (msg, source) in &sources {
            let expected = if msg == "Kp 5.0" { "global" } else { "rule" };
            assert_eq!(source, expected, "{msg} should be {expected}");
        }

        let alice = msgs("alice@example.com");
        assert!(alice.iter().any(|m| m == "Kp 5.0"), "global must be visible");
        assert!(alice.iter().any(|m| m == "alice rule"), "own rule must be visible");
        assert!(
            !alice.iter().any(|m| m.contains("bob")),
            "another account's rule must not appear: {alice:?}"
        );

        let bob = msgs("bob@example.com");
        assert!(bob.iter().any(|m| m == "Kp 5.0"));
        assert!(bob.iter().any(|m| m == "bob secret threshold"));
        assert!(!bob.iter().any(|m| m.contains("alice")));

        // An account with no rules of its own still sees the global feed.
        let stranger = msgs("nobody@example.com");
        assert_eq!(stranger, vec!["Kp 5.0".to_string()]);
    }

    /// Every read path over `alerts_anomaly` must show an account the global
    /// detections plus its own rules and nothing else. The three are asserted
    /// together on purpose.
    ///
    /// AUD-008 was fixed in `get_anomalies_recent` and left open in
    /// `get_events_page` for three weeks, even though the finding named both in
    /// the same list, because the rule was written into one query instead of
    /// into one test. `get_report_summary` counted every account's rule firings
    /// into the number each account reads as its own. A new reader of this table
    /// belongs in this test.
    #[test]
    fn every_anomaly_read_path_is_scoped_to_the_caller() {
        let store = mem_store();
        store
            .insert_anomaly("kp_storm", "g1", "warning", "Kp 5.0", None)
            .expect("global");
        store
            .insert_anomaly("custom:r1", "r1:1", "warning", "alice rule", Some("alice@example.com"))
            .expect("alice");
        store
            .insert_anomaly("custom:r2", "r2:1", "critical", "bob secret threshold", Some("bob@example.com"))
            .expect("bob");

        // 1. /api/anomalies and the MCP get_anomalies tool.
        let feed: Vec<String> = store
            .get_anomalies_recent("alice@example.com")
            .expect("read")
            .as_array()
            .expect("array")
            .iter()
            .map(|v| v["message"].as_str().unwrap_or_default().to_string())
            .collect();
        assert!(feed.iter().any(|m| m == "Kp 5.0"));
        assert!(feed.iter().any(|m| m == "alice rule"));
        assert!(!feed.iter().any(|m| m.contains("bob")), "{feed:?}");

        // 2. /api/events, which was the second door.
        let page = store
            .get_events_page("alice@example.com", 0, None, None, 1, 100)
            .expect("events");
        let events: Vec<String> = page["events"]
            .as_array()
            .expect("array")
            .iter()
            .map(|v| v["message"].as_str().unwrap_or_default().to_string())
            .collect();
        assert!(events.iter().any(|m| m == "Kp 5.0"), "{events:?}");
        assert!(events.iter().any(|m| m == "alice rule"), "{events:?}");
        assert!(
            !events.iter().any(|m| m.contains("bob")),
            "another account's rule reached the events page: {events:?}"
        );
        // The total drives pagination, so it has to be scoped as well or the
        // page count advertises rows the caller can never receive.
        assert_eq!(page["total"].as_i64(), Some(2), "total must be scoped too");

        // 3. /api/reports/summary, which reports a count rather than the rows.
        let count = |email: &str| -> i64 {
            store
                .get_report_summary(email, 86_400)
                .expect("summary")["anomaly_count"]
                .as_i64()
                .expect("count")
        };
        assert_eq!(count("alice@example.com"), 2);
        assert_eq!(count("bob@example.com"), 2);
        assert_eq!(count("nobody@example.com"), 1, "a stranger sees globals only");
    }

    /// A noisy rule filled the shared limit and pushed global anomalies out of
    /// everyone's feed. The two windows are taken separately so it cannot.
    #[test]
    fn a_noisy_rule_cannot_evict_global_anomalies() {
        let store = mem_store();
        for i in 0..200 {
            store
                .insert_anomaly(
                    "custom:noisy",
                    &format!("noisy:{i}"),
                    "warning",
                    "noise",
                    Some("noisy@example.com"),
                )
                .expect("noise");
        }
        store
            .insert_anomaly("kp_storm", "g1", "critical", "Kp 8.0", None)
            .expect("global");

        let out = store.get_anomalies_recent("noisy@example.com").expect("read");
        let rows = out.as_array().expect("array");
        assert!(
            rows.iter().any(|v| v["message"] == "Kp 8.0"),
            "the global anomaly must survive 200 rows of one rule"
        );
        // Own rows are capped separately, so the response stays bounded.
        let own = rows.iter().filter(|v| v["message"] == "noise").count();
        assert_eq!(own, 50, "own anomalies are windowed at 50, got {own}");
    }

    /// A stored second factor must not be readable from the row itself. The
    /// whole point is that a database dump does not hand over 2FA alongside the
    /// password hashes it sits next to.
    #[test]
    fn a_stored_second_factor_is_encrypted_at_rest() {
        let key = "1".repeat(64);
        // SAFETY: single threaded test.
        unsafe { std::env::set_var("TOTP_ENCRYPTION_KEY", &key) };
        let store = mem_store();
        store.create_user("tot@example.com", "hash").expect("user");
        store
            .set_totp_secret("tot@example.com", "JBSWY3DPEHPK3PXP")
            .expect("store secret");

        let raw: String = store
            .conn
            .query_row(
                "SELECT totp_secret_enc FROM users WHERE email = ?",
                params!["tot@example.com"],
                |r| r.get(0),
            )
            .expect("raw column");
        assert!(raw.starts_with("v1:"), "stored as {raw}");
        assert!(
            !raw.contains("JBSWY3DPEHPK3PXP"),
            "the plaintext secret must not be in the row"
        );

        let user = store
            .find_user_by_email("tot@example.com")
            .expect("lookup")
            .expect("user exists");
        assert_eq!(
            store.totp_secret(&user).expect("decrypt"),
            Some("JBSWY3DPEHPK3PXP".to_string())
        );

        // Clearing removes it entirely.
        store.disable_totp("tot@example.com").expect("clear");
        let user = store
            .find_user_by_email("tot@example.com")
            .expect("lookup")
            .expect("user exists");
        assert_eq!(store.totp_secret(&user).expect("decrypt"), None);
        unsafe { std::env::remove_var("TOTP_ENCRYPTION_KEY") };
    }

    /// With no key configured, enrolling must fail rather than fall back to
    /// storing the secret in the clear.
    #[test]
    fn without_a_key_a_second_factor_cannot_be_stored() {
        // SAFETY: single threaded test.
        unsafe { std::env::remove_var("TOTP_ENCRYPTION_KEY") };
        let store = mem_store();
        store.create_user("nokey@example.com", "hash").expect("user");
        assert!(matches!(
            store.set_totp_secret("nokey@example.com", "JBSWY3DPEHPK3PXP"),
            Err(DbError::EncryptionUnavailable)
        ));
    }

    /// The plaintext column is gone, so there is no place left for a secret to
    /// be written unencrypted by mistake.
    #[test]
    fn the_plaintext_totp_column_no_longer_exists() {
        let store = mem_store();
        let n: i64 = store
            .conn
            .query_row(
                "SELECT COUNT(*) FROM information_schema.columns \
                 WHERE table_name = 'users' AND column_name = 'totp_secret'",
                [],
                |r| r.get(0),
            )
            .expect("column lookup");
        assert_eq!(n, 0, "users.totp_secret must have been dropped");
    }

    /// Each period keeps its own row. One row per user meant every new period
    /// overwrote the last, so no billing history existed.
    #[test]
    fn usage_is_recorded_per_period() {
        let store = mem_store();
        let day = 86_400;
        for (start, count) in [(day, 10), (day * 2, 20), (day * 3, 30)] {
            store
                .upsert_usage_record("u@example.com", count, start, start + day)
                .expect("record");
        }

        assert_eq!(
            store.get_usage_for_period("u@example.com", day * 2).expect("read"),
            Some((20, day * 2, day * 3))
        );

        let history = store.list_usage_history("u@example.com", 24).expect("history");
        assert_eq!(history.len(), 3, "every period must survive");
        assert_eq!(history[0].1, day * 3, "newest first");
        assert_eq!(history[2].1, day, "oldest last");

        // A later flush for the same period corrects it rather than adding a row.
        store
            .upsert_usage_record("u@example.com", 25, day * 2, day * 3)
            .expect("correct");
        assert_eq!(
            store.get_usage_for_period("u@example.com", day * 2).expect("read"),
            Some((25, day * 2, day * 3))
        );
        assert_eq!(store.list_usage_history("u@example.com", 24).expect("history").len(), 3);
    }

    /// A dashboard only account is known to the counter, because the session
    /// path caches its token version there, but spends no quota. Writing it an
    /// empty row every period would accrue one per account per period forever.
    #[test]
    fn a_zero_count_never_creates_a_row() {
        let store = mem_store();
        let day = 86_400;
        for period in 1..=5 {
            store
                .upsert_usage_record("dash@example.com", 0, day * period, day * (period + 1))
                .expect("flush");
        }
        assert!(
            store.list_usage_history("dash@example.com", 24).expect("history").is_empty(),
            "no rows should exist for an account that spent nothing"
        );
        // And reading such a period reports zero rather than failing.
        assert_eq!(
            store.get_usage_for_period("dash@example.com", day).expect("read"),
            None
        );
    }

    /// A row that already exists must still be updated when the count is zero,
    /// so a correction downward is possible and nothing recorded disappears.
    #[test]
    fn a_zero_count_updates_an_existing_row_rather_than_deleting_it() {
        let store = mem_store();
        let day = 86_400;
        store
            .upsert_usage_record("u@example.com", 42, day, day * 2)
            .expect("record");
        assert_eq!(
            store.get_usage_for_period("u@example.com", day).expect("read"),
            Some((42, day, day * 2))
        );

        store
            .upsert_usage_record("u@example.com", 0, day, day * 2)
            .expect("correct to zero");

        let stored = store.get_usage_for_period("u@example.com", day).expect("read");
        assert_eq!(
            stored,
            Some((0, day, day * 2)),
            "the row must survive and read zero, not vanish"
        );
        assert_eq!(
            store.list_usage_history("u@example.com", 24).expect("history").len(),
            1
        );
    }

    /// One account's usage must not appear under another's.
    #[test]
    fn usage_history_is_scoped_to_one_account() {
        let store = mem_store();
        let day = 86_400;
        store.upsert_usage_record("a@example.com", 5, day, day * 2).expect("a");
        store.upsert_usage_record("b@example.com", 7, day, day * 2).expect("b");
        assert_eq!(
            store.get_usage_for_period("a@example.com", day).expect("read"),
            Some((5, day, day * 2))
        );
        assert_eq!(store.list_usage_history("b@example.com", 24).expect("history").len(), 1);
    }

    /// The defect: two satellites reporting the same minute and band collided on
    /// a key of (time_tag, energy), and ON CONFLICT DO NOTHING dropped whichever
    /// arrived second. Which one survived depended on arrival order.
    #[test]
    fn two_satellites_reporting_the_same_minute_both_survive() {
        let store = mem_store();
        let t = iso(now() - 60);
        store
            .insert_xray_batch(&[
                XRayRecord {
                    time_tag: t.clone(),
                    satellite: 16,
                    flux: 1.0e-6,
                    observed_flux: 1.1e-6,
                    energy: "0.1-0.8nm".into(),
                },
                XRayRecord {
                    time_tag: t.clone(),
                    satellite: 18,
                    flux: 2.0e-6,
                    observed_flux: 2.1e-6,
                    energy: "0.1-0.8nm".into(),
                },
            ])
            .expect("insert both");

        let n: i64 = store
            .conn
            .query_row(
                "SELECT COUNT(*) FROM xray WHERE time_tag = ? AND energy = '0.1-0.8nm'",
                params![t],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(n, 2, "both satellites must be stored, not one");

        let sats: Vec<i32> = {
            let mut stmt = store
                .conn
                .prepare("SELECT satellite FROM xray WHERE time_tag = ? ORDER BY satellite")
                .expect("prepare");
            stmt.query_map(params![t], |r| r.get(0))
                .expect("query")
                .collect::<Result<Vec<_>, _>>()
                .expect("collect")
        };
        assert_eq!(sats, vec![16, 18]);
    }

    /// The starter tier existed only in the backend, ranked and priced the same
    /// as free, and the pricing page never sold it. Existing accounts move
    /// rather than being left naming a tier nothing recognises.
    #[test]
    fn existing_starter_accounts_are_moved_to_free() {
        let dir = std::env::temp_dir().join(format!("starter-{}", now()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("legacy.duckdb");
        let path_str = path.to_string_lossy().to_string();

        {
            let conn = Connection::open(&path_str).expect("open");
            conn.execute_batch(
                "CREATE TABLE users (
                     email TEXT NOT NULL PRIMARY KEY,
                     password_hash TEXT NOT NULL,
                     created_at BIGINT NOT NULL,
                     plan TEXT DEFAULT 'starter'
                 );
                 INSERT INTO users VALUES
                     ('old@example.com', 'hash', 1767225600, 'starter'),
                     ('paid@example.com', 'hash', 1767225600, 'pro');",
            )
            .expect("seed");
        }

        let store = Store::open(&path_str).expect("open through the migration");
        assert_eq!(store.get_user_plan("old@example.com").expect("plan"), "free");
        assert_eq!(
            store.get_user_plan("paid@example.com").expect("plan"),
            "pro",
            "a paid tier must not be touched"
        );

        // A new account gets free, not starter.
        store.create_user("new@example.com", "hash").expect("create");
        assert_eq!(store.get_user_plan("new@example.com").expect("plan"), "free");

        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The publication lag is the reason the limit is nine hours, not six. A
    /// value covering 21:00 appears around 00:00, so at 05:00 the newest stored
    /// value is eight hours old and the series is healthy. Six hours called that
    /// stale and took the forecast down with it.
    #[test]
    fn kp_3h_tolerates_the_noaa_publishing_lag() {
        let store = mem_store();
        // Eight hours old: normal for this series near the end of a cycle.
        store
            .insert_kp_3h_batch(&[Kp3hRecord {
                time_tag: iso(now() - 8 * 3_600),
                kp: 2.0,
            }])
            .unwrap();
        let health = store.series_health();
        let status = |c: &str| {
            health
                .iter()
                .find(|(component, _, _)| *component == c)
                .map(|(_, s, _)| *s)
                .expect("component")
        };
        assert_eq!(
            status("noaa_kp_3h"),
            "operational",
            "an eight hour old three hourly value is normal, not stale"
        );
        assert!(
            !store.get_kp_3h_recent().unwrap().as_array().unwrap().is_empty(),
            "and it must still be served"
        );

        // Ten hours old is past the limit and does count as stale.
        let store = mem_store();
        store
            .insert_kp_3h_batch(&[Kp3hRecord {
                time_tag: iso(now() - 10 * 3_600),
                kp: 2.0,
            }])
            .unwrap();
        let health = store.series_health();
        let stale = health
            .iter()
            .find(|(component, _, _)| *component == "noaa_kp_3h")
            .map(|(_, s, _)| *s)
            .expect("component");
        assert_eq!(stale, "degraded", "ten hours is a genuinely stopped feed");
    }

    /// Kyoto publishes Dst provisionally and corrects it later. Both the
    /// incremental filter and the conflict clause used to discard the
    /// correction, so the stored series was frozen at first sight.
    #[test]
    fn a_corrected_dst_value_replaces_the_provisional_one() {
        let store = mem_store();
        // Bound once. Calling now() per use let a second tick over between the
        // insert and the read, which made the lookup miss under load.
        let base = now();
        let hour = |offset: i64| iso(base - offset * 3_600);

        // First poll: three hours of provisional values.
        store
            .insert_dst_batch(&[
                DstRecord { time_tag: hour(3), dst_nt: Some(-20) },
                DstRecord { time_tag: hour(2), dst_nt: Some(-30) },
                DstRecord { time_tag: hour(1), dst_nt: Some(-40) },
            ])
            .expect("first poll");

        // Second poll: the same window, two hours corrected, one hour new. This
        // is exactly the shape the live feed returns.
        store
            .insert_dst_batch(&[
                DstRecord { time_tag: hour(3), dst_nt: Some(-22) },
                DstRecord { time_tag: hour(2), dst_nt: Some(-30) },
                DstRecord { time_tag: hour(1), dst_nt: Some(-45) },
                DstRecord { time_tag: hour(0), dst_nt: Some(-51) },
            ])
            .expect("second poll");

        let read = |tag: String| -> Option<i32> {
            store
                .conn
                .query_row(
                    "SELECT dst_nt FROM dst WHERE time_tag = ?",
                    params![tag],
                    |r| r.get(0),
                )
                .ok()
        };
        assert_eq!(read(hour(3)), Some(-22), "a corrected hour must be updated");
        assert_eq!(read(hour(2)), Some(-30), "an unchanged hour stays as it was");
        assert_eq!(read(hour(1)), Some(-45), "the most recent hour corrects too");
        assert_eq!(read(hour(0)), Some(-51), "a new hour is still inserted");

        let n: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM dst", [], |r| r.get(0))
            .expect("count");
        assert_eq!(n, 4, "corrections must update rows, not add them");
    }

    /// The old behaviour, stated so a reintroduced filter fails here: an hour at
    /// or below the newest stored one used to be dropped before the insert ran.
    #[test]
    fn an_older_dst_hour_is_no_longer_filtered_out() {
        let store = mem_store();
        let newest = iso(now());
        let older = iso(now() - 7_200);

        store
            .insert_dst_batch(&[DstRecord { time_tag: newest.clone(), dst_nt: Some(-10) }])
            .expect("newest first");
        // Arrives after a newer hour is already stored, which the incremental
        // filter would have discarded.
        store
            .insert_dst_batch(&[DstRecord { time_tag: older.clone(), dst_nt: Some(-99) }])
            .expect("older hour");

        let stored: Option<i32> = store
            .conn
            .query_row(
                "SELECT dst_nt FROM dst WHERE time_tag = ?",
                params![older],
                |r| r.get(0),
            )
            .ok();
        assert_eq!(stored, Some(-99), "an older hour must still be written");
    }

    /// A revised reading replaces the first one, within a batch. NOAA applies
    /// corrections, and first-write-wins pinned whatever arrived first.
    #[test]
    fn a_revised_xray_reading_replaces_the_original() {
        let store = mem_store();
        let t = iso(now() - 60);
        let reading = |flux: f64, observed: f64| XRayRecord {
            time_tag: t.clone(),
            satellite: 18,
            flux,
            observed_flux: observed,
            energy: "0.1-0.8nm".into(),
        };

        store
            .insert_xray_batch(&[reading(1.0e-6, 1.1e-6), reading(3.0e-6, 3.3e-6)])
            .expect("insert");

        let (flux, observed, n): (i64, i64, i64) = store
            .conn
            .query_row(
                "SELECT flux_e12, observed_flux_e12, (SELECT COUNT(*) FROM xray) FROM xray",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .expect("read back");
        assert_eq!(n, 1, "a revision must not add a row");
        assert_eq!(flux, 3_000_000, "the revised flux must win");
        assert_eq!(observed, 3_300_000, "the revised observed flux must win");
    }

    /// A revision arriving in a LATER batch never reaches the conflict clause,
    /// because insert_xray_batch drops everything at or below MAX(time_tag)
    /// before inserting. This pins the limitation rather than leaving it to be
    /// rediscovered: DO UPDATE fixes revisions within one fetch, and the
    /// incremental filter is what blocks them across fetches.
    #[test]
    fn a_revision_in_a_later_batch_is_dropped_by_the_incremental_filter() {
        let store = mem_store();
        let t = iso(now() - 60);
        let reading = |flux: f64| XRayRecord {
            time_tag: t.clone(),
            satellite: 18,
            flux,
            observed_flux: flux,
            energy: "0.1-0.8nm".into(),
        };

        store.insert_xray_batch(&[reading(1.0e-6)]).expect("first");
        store.insert_xray_batch(&[reading(3.0e-6)]).expect("later revision");

        let flux: i64 = store
            .conn
            .query_row("SELECT flux_e12 FROM xray", [], |r| r.get(0))
            .expect("read back");
        assert_eq!(
            flux, 1_000_000,
            "known limitation: the incremental filter drops the later revision"
        );
    }

    /// Re-fetching the same measurement is not a duplicate. The poller re-reads
    /// the whole one day window every two minutes.
    #[test]
    fn refetching_the_same_reading_does_not_duplicate_it() {
        let store = mem_store();
        let t = iso(now() - 60);
        let rec = || XRayRecord {
            time_tag: t.clone(),
            satellite: 18,
            flux: 1.0e-6,
            observed_flux: 1.1e-6,
            energy: "0.1-0.8nm".into(),
        };
        store.insert_xray_batch(&[rec()]).expect("first");
        store.insert_xray_batch(&[rec()]).expect("second");
        let n: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM xray", [], |r| r.get(0))
            .expect("count");
        assert_eq!(n, 1);
    }

    /// The rebuild carries every existing row. Losing 262k rows of flux to gain
    /// a trustworthy label on a column nothing renders would be a bad trade.
    #[test]
    fn the_xray_rebuild_keeps_every_existing_row() {
        let dir = std::env::temp_dir().join(format!("xray-rebuild-{}", now()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("legacy.duckdb");
        let path_str = path.to_string_lossy().to_string();

        // A database in the shape that predates the migration: old primary key,
        // and no schema_migrations row for it.
        {
            let conn = Connection::open(&path_str).expect("open");
            conn.execute_batch(
                "CREATE TABLE xray (
                     time_tag TEXT NOT NULL, energy TEXT NOT NULL, satellite INTEGER NOT NULL,
                     flux_e12 BIGINT NOT NULL, observed_flux_e12 BIGINT NOT NULL,
                     observed_at BIGINT, fetched_at BIGINT NOT NULL,
                     PRIMARY KEY (time_tag, energy)
                 );
                 INSERT INTO xray VALUES
                     ('2026-01-01T00:00:00Z', '0.1-0.8nm',  16, 100, 110, 1767225600, 1767225600),
                     ('2026-01-01T00:01:00Z', '0.1-0.8nm',  16, 200, 210, 1767225660, 1767225660),
                     ('2026-01-01T00:01:00Z', '0.05-0.4nm', 16, 300, 310, 1767225660, 1767225660);",
            )
            .expect("seed");
        }

        let store = Store::open(&path_str).expect("open through the migration");
        let n: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM xray", [], |r| r.get(0))
            .expect("count");
        assert_eq!(n, 3, "every pre-existing row must survive the rebuild");

        // And the new key is in force: the same minute under a second satellite
        // now lands instead of being dropped.
        store
            .conn
            .execute(
                "INSERT INTO xray VALUES
                     ('2026-01-01T00:00:00Z', '0.1-0.8nm', 18, 999, 999, 1767225600, 1767225600)",
                [],
            )
            .expect("second satellite must be accepted");
        let n: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM xray", [], |r| r.get(0))
            .expect("count");
        assert_eq!(n, 4);

        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn test_apod(date: &str) -> crate::nasa::Apod {
        crate::nasa::Apod {
            date: date.to_owned(),
            title: "Test".into(),
            explanation: "Test".into(),
            url: "https://example.invalid/a.jpg".into(),
            media_type: "image".into(),
            hdurl: None,
        }
    }

    fn test_epic(identifier: &str) -> crate::nasa::EpicImage {
        crate::nasa::EpicImage {
            identifier: identifier.to_owned(),
            caption: "Test".into(),
            image: "epic_test".into(),
            date: "2026-08-10 00:00:00".into(),
            centroid_coordinates: crate::nasa::CentroidCoordinates { lat: 1.0, lon: 2.0 },
        }
    }

    /// The nasa component used to be one aggregate over apod, neo and epic, so
    /// APOD arriving daily kept it green while the other two were dead. That is
    /// the same fault the NOAA split fixed, and each feed now answers for itself.
    #[test]
    fn each_nasa_feed_reports_its_own_freshness() {
        let store = mem_store();
        store.insert_apod(&test_apod("2026-08-10")).unwrap();
        store.insert_epic_batch(&[test_epic("epic-1")]).unwrap();
        // Age only the EPIC write, well past its limit.
        store
            .conn
            .execute_batch("UPDATE epic SET fetched_at = fetched_at - 100000")
            .unwrap();

        let health = store.series_health();
        let status = |c: &str| {
            health
                .iter()
                .find(|(component, _, _)| *component == c)
                .map(|(_, s, _)| *s)
                .unwrap_or("missing")
        };
        // The live feed must not cover for the dead one.
        assert_eq!(status("nasa_apod"), "operational");
        assert_eq!(status("nasa_epic"), "degraded");
        // Never written, so unknown rather than a false green.
        assert_eq!(status("nasa_neo"), "unknown");
        assert_eq!(status("nasa_exoplanets"), "unknown");
    }

    /// `fetched_at` has to mean "the last poll that returned rows", not "the
    /// first time this row appeared". With ON CONFLICT DO NOTHING it only moved
    /// when something new arrived, so a quiet week in the exoplanet archive was
    /// indistinguishable from a dead poller.
    #[test]
    fn a_repeat_poll_refreshes_fetched_at() {
        let store = mem_store();
        store.insert_apod(&test_apod("2026-08-10")).unwrap();
        store
            .conn
            .execute_batch("UPDATE apod SET fetched_at = fetched_at - 100000")
            .unwrap();

        // The same row again, which is what every poll returns until tomorrow.
        store.insert_apod(&test_apod("2026-08-10")).unwrap();

        let newest: i64 = store
            .conn
            .query_row("SELECT MAX(fetched_at) FROM apod", [], |r| r.get(0))
            .unwrap();
        assert!(
            now() - newest < 5,
            "a repeat poll must refresh fetched_at, or a working poller reads as dead"
        );
        let rows: i64 = store
            .conn
            .query_row("SELECT count(*) FROM apod", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1, "the upsert must not duplicate the row");
    }

    /// The asteroid warning is about approaches that are coming, not rows that
    /// arrived recently.
    ///
    /// It filtered on `fetched_at` inside a backward window, which excluded
    /// nothing in practice because the poller refetches every thirty minutes,
    /// and would have reported last week's approach as a current warning
    /// (AUD-024).
    #[test]
    fn the_asteroid_window_is_the_approach_not_the_ingest() {
        let store = mem_store();
        let day = |offset: i64| {
            chrono::DateTime::from_timestamp(now() + offset * 86_400, 0)
                .expect("timestamp")
                .format("%Y-%m-%d")
                .to_string()
        };
        let close = 100_000_000i64; // well inside one lunar distance

        for (id, offset) in [
            ("passed-last-week", -7i64),
            ("passed-yesterday", -1),
            ("today", 0),
            ("in-three-days", 3),
            ("in-seven-days", 7),
            ("in-ten-days", 10),
        ] {
            store
                .conn
                .execute(
                    "INSERT INTO neo (id, close_approach_date, name, is_hazardous, \
                     diameter_min_m, diameter_max_m, velocity_m_per_h, miss_distance_m, fetched_at) \
                     VALUES (?, ?, 'test', false, 1, 2, 3, ?, ?)",
                    params![id, day(offset), close, now()],
                )
                .expect("insert");
        }
        // Far away and imminent: distance still decides, independently of date.
        store
            .conn
            .execute(
                "INSERT INTO neo (id, close_approach_date, name, is_hazardous, \
                 diameter_min_m, diameter_max_m, velocity_m_per_h, miss_distance_m, fetched_at) \
                 VALUES ('far-but-soon', ?, 'test', false, 1, 2, 3, ?, ?)",
                params![day(1), 900_000_000_000i64, now()],
            )
            .expect("insert");

        let found: Vec<String> = store
            .neo_close_approaches_raw(ONE_LD_SCALED_FOR_TEST, 7)
            .expect("query")
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();

        assert!(found.contains(&"today".to_string()), "an approach today counts");
        assert!(found.contains(&"in-three-days".to_string()));
        assert!(found.contains(&"in-seven-days".to_string()), "the horizon is inclusive");
        assert!(
            !found.contains(&"passed-yesterday".to_string()),
            "an approach that already happened is not a warning"
        );
        assert!(!found.contains(&"passed-last-week".to_string()));
        assert!(
            !found.contains(&"in-ten-days".to_string()),
            "beyond the poller's own seven day window there is no data to trust"
        );
        assert!(!found.contains(&"far-but-soon".to_string()), "distance still decides");
    }

    /// An alert claims conditions have exceeded a threshold, present tense, so a
    /// reading old enough to be wrong about now must not produce one (AUD-028).
    /// The limit is the series' own, not a second number kept beside it.
    #[test]
    fn a_reading_is_current_only_within_its_own_series_limit() {
        let now = 1_800_000_000;

        // noaa_kp allows 1800s. The boundary is inclusive on the fresh side.
        assert!(reading_is_current("noaa_kp", now - 1_799, now));
        assert!(reading_is_current("noaa_kp", now - 1_800, now));
        assert!(!reading_is_current("noaa_kp", now - 1_801, now));

        // noaa_kp_3h allows nine hours, because NOAA publishes it late. Reading
        // the limit from the table rather than hardcoding one is what keeps the
        // two from disagreeing.
        assert!(reading_is_current("noaa_kp_3h", now - 32_400, now));
        assert!(!reading_is_current("noaa_kp_3h", now - 32_401, now));

        // Something the table has never heard of gets the safe answer.
        assert!(!reading_is_current("not_a_component", now, now));

        // And every series the alerts read from is actually in the table, so a
        // rename cannot silently turn the check into "always stale".
        for component in ["noaa_kp", "noaa_solar_wind"] {
            assert!(
                SERIES_FRESHNESS.iter().any(|s| s.component == component),
                "{component} must have a freshness entry for the alert bound to mean anything"
            );
        }
    }

    /// Every write that changes how an account authenticates must invalidate
    /// tokens minted before it.
    ///
    /// Declared as a list rather than asserted one function at a time, because
    /// the failure here is a fourth writer arriving without the bump. A
    /// password change had it, the two second factor writes did not, so
    /// enabling 2FA against a stolen session left the thief holding a working
    /// token for up to twenty four hours (AUD-018).
    ///
    /// `set_totp_secret` is deliberately absent: it stores a secret during
    /// setup while `totp_enabled` is still false, so nothing about how the
    /// account authenticates has changed yet. `verify_2fa` is what flips it,
    /// and that goes through `enable_totp`.
    #[test]
    fn every_factor_change_invalidates_sessions() {
        type Op = fn(&Store, &str) -> Result<(), DbError>;
        let operations: [(&str, Op); 3] = [
            ("a password change", |s, e| s.update_password_hash(e, "new-hash")),
            ("enabling the second factor", |s, e| s.enable_totp(e)),
            ("disabling the second factor", |s, e| s.disable_totp(e)),
        ];

        for (name, operation) in operations {
            let store = mem_store();
            let email = "factor@example.com";
            store.create_user(email, "hash").unwrap();
            let before = store.get_token_version(email).unwrap();

            operation(&store, email).unwrap();

            assert_eq!(
                store.get_token_version(email).unwrap(),
                before + 1,
                "{name} must invalidate sessions minted before it"
            );
        }
    }

    /// The alerts feed reports on the poll that fetched it, not on the age of
    /// what it fetched, and the recorded verdict has to expire.
    ///
    /// A poller that stops writing would otherwise leave its last good answer
    /// standing forever, which is the same failure as a chart drawing a dead
    /// feed's final day as if it were current.
    #[test]
    fn poll_liveness_expires_a_verdict_nobody_is_refreshing() {
        let store = mem_store();
        let status = |s: &Store| {
            s.poll_liveness()
                .iter()
                .find(|(component, _, _)| *component == "noaa_alerts")
                .map(|(_, st, _)| *st)
                .expect("noaa_alerts is declared in POLL_LIVENESS")
        };

        // Nothing recorded yet is not a fault. It is a backend that has not
        // polled since it started.
        assert_eq!(status(&store), "unknown");

        store
            .insert_health_snapshot("noaa_alerts", now() - 60, "operational")
            .unwrap();
        assert_eq!(status(&store), "operational");

        store
            .insert_health_snapshot("noaa_alerts", now() - 30, "degraded")
            .unwrap();
        assert_eq!(status(&store), "degraded", "the newest verdict wins");

        // Six missed cycles. The last verdict was operational and it no longer
        // means anything.
        let store = mem_store();
        store
            .insert_health_snapshot("noaa_alerts", now() - 3_600, "operational")
            .unwrap();
        assert_eq!(status(&store), "degraded");
    }

    /// One live series must not cover for a dead one. The status page read a
    /// single Kp query and called the whole of NOAA healthy while the
    /// magnetometer feed had been dead for forty days.
    #[test]
    fn series_health_reports_each_series_on_its_own() {
        let store = mem_store();
        store
            .insert_kp_batch(&[KpRecord {
                time_tag: iso(now() - 60),
                kp_index: 2,
                estimated_kp: 2.33,
            }])
            .unwrap();
        store
            .insert_imf_batch(&[ImfRecord {
                time_tag: iso(now() - 40 * 86_400),
                bz_gsm: Some(1.01),
                bt: Some(14.71),
            }])
            .unwrap();

        let health = store.series_health();
        let status = |c: &str| {
            health
                .iter()
                .find(|(component, _, _)| *component == c)
                .map(|(_, s, _)| *s)
                .unwrap()
        };
        assert_eq!(status("noaa_kp"), "operational");
        assert_eq!(status("noaa_imf"), "degraded");
        // Never written, so its state is unknown rather than a false green.
        assert_eq!(status("noaa_xray"), "unknown");
        assert_eq!(health.len(), SERIES_FRESHNESS.len());
    }

    /// Kyoto publishes provisional Dst a day or more late. A reading that is
    /// stale for every other series is normal for Dst, and the looser limit is
    /// what stops the status page reporting degraded every day.
    #[test]
    fn dst_tolerates_the_kyoto_publishing_lag() {
        let store = mem_store();
        let a_day_ago = iso(now() - 86_400);
        store
            .insert_dst_batch(&[DstRecord {
                time_tag: a_day_ago.clone(),
                dst_nt: Some(-45),
            }])
            .unwrap();
        store
            .insert_imf_batch(&[ImfRecord {
                time_tag: a_day_ago,
                bz_gsm: Some(1.01),
                bt: Some(14.71),
            }])
            .unwrap();

        let health = store.series_health();
        let status = |c: &str| {
            health
                .iter()
                .find(|(component, _, _)| *component == c)
                .map(|(_, s, _)| *s)
                .unwrap()
        };
        assert_eq!(status("noaa_dst"), "operational");
        assert_eq!(status("noaa_imf"), "degraded");
        assert_eq!(store.get_dst_recent().unwrap().as_array().unwrap().len(), 1);
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
        // Ends at the current minute, because a stale series reads as empty.
        let base = now() - 1499 * 60;

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
        let base3 = now() - 299 * 10_800;
        let kp3: Vec<Kp3hRecord> = (0..300)
            .map(|i| Kp3hRecord {
                time_tag: iso(base3 + i * 10_800),
                kp: 2.0,
            })
            .collect();
        store.insert_kp_3h_batch(&kp3).unwrap();

        let out3 = store.get_kp_3h_recent().unwrap();
        let arr3 = out3.as_array().unwrap();
        assert_eq!(arr3.len(), 240);
        assert_eq!(
            arr3[0]["time_tag"].as_str().unwrap(),
            iso(base3 + 60 * 10_800)
        );
        assert_eq!(
            arr3[239]["time_tag"].as_str().unwrap(),
            iso(base3 + 299 * 10_800)
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
        let summary = store.get_report_summary("reader@example.com", 24 * 3600).unwrap();
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
            store.get_report_summary("reader@example.com", 48 * 3600).unwrap()["kp_count"]
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

    /// A full window of stale readings must fail, not forecast. The model would
    /// otherwise take readings that stopped arriving weeks ago and return a
    /// prediction that reads as current.
    #[test]
    fn stale_kp_3h_history_errors_instead_of_forecasting_from_old_readings() {
        let store = mem_store();
        let seq_len = 16;

        // A complete window, but it ends forty days ago.
        let frozen = now() - 40 * 86_400;
        let stale: Vec<Kp3hRecord> = (0..seq_len)
            .map(|i| Kp3hRecord {
                time_tag: iso(frozen + i as i64 * 10_800),
                kp: 3.0,
            })
            .collect();
        store.insert_kp_3h_batch(&stale).unwrap();

        match store.get_recent_kp_3h(seq_len) {
            Err(DbError::StaleSeries {
                series,
                newest_observed_at,
            }) => {
                assert_eq!(series, "kp_3h");
                assert_eq!(
                    newest_observed_at,
                    Some(frozen + (seq_len as i64 - 1) * 10_800)
                );
            }
            other => panic!("expected StaleSeries, got {other:?}"),
        }

        // A current window of the same length forecasts normally.
        let fresh_base = now() - (seq_len as i64 - 1) * 10_800;
        let fresh: Vec<Kp3hRecord> = (0..seq_len)
            .map(|i| Kp3hRecord {
                time_tag: iso(fresh_base + i as i64 * 10_800),
                kp: 4.5,
            })
            .collect();
        store.insert_kp_3h_batch(&fresh).unwrap();

        let seq = store
            .get_recent_kp_3h(seq_len)
            .expect("a current window still yields a sequence");
        assert_eq!(seq.len(), seq_len);
        assert!((seq[seq_len - 1] - 4.5).abs() < 1e-9);
    }

    /// Staleness is measured against the limit SERIES_FRESHNESS already holds
    /// for this series, so there is only ever one number to change.
    #[test]
    fn stale_kp_3h_uses_the_shared_series_limit() {
        let limit = SERIES_FRESHNESS
            .iter()
            .find(|s| s.table == "kp_3h")
            .map(|s| s.max_age_secs)
            .expect("kp_3h has a freshness limit");

        let seq_len = 8;
        let build = |newest_age: i64| {
            let store = mem_store();
            let base = now() - newest_age - (seq_len as i64 - 1) * 10_800;
            let rows: Vec<Kp3hRecord> = (0..seq_len)
                .map(|i| Kp3hRecord {
                    time_tag: iso(base + i as i64 * 10_800),
                    kp: 3.0,
                })
                .collect();
            store.insert_kp_3h_batch(&rows).unwrap();
            store.get_recent_kp_3h(seq_len)
        };

        assert!(build(limit - 60).is_ok(), "inside the limit must forecast");
        assert!(
            matches!(build(limit + 60), Err(DbError::StaleSeries { .. })),
            "past the limit must be the staleness error"
        );
    }

    /// A short history must be an error, never a short vector: the ML service
    /// would pad the shortfall and forecast from mostly synthetic input.
    #[test]
    fn short_kp_3h_history_errors_instead_of_returning_a_short_sequence() {
        let store = mem_store();
        let seq_len = 16;
        // Ends at the current period, so this test sees the short history error
        // and not the staleness error.
        let base = now() - (seq_len as i64 - 1) * 10_800;

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

    fn sat(norad_id: i32) -> StarlinkSat {
        StarlinkSat {
            norad_id,
            name: format!("STARLINK-{norad_id}"),
            tle_line1: format!("1 {norad_id:05}U 24001A   26001.00000000  .00000000  00000-0  00000-0 0  9990"),
            tle_line2: format!("2 {norad_id:05}  53.0000   0.0000 0001000   0.0000   0.0000 15.00000000    00"),
        }
    }

    /// The starlink table is the only full replace in the schema, so its empty
    /// batch guard is data protection rather than an optimisation.
    ///
    /// Celestrak answers 403 "GP data has not updated" on roughly every other
    /// poll, which reaches the writer as an empty batch. Delete the
    /// `sats.is_empty()` early return in insert_starlink_batch and the
    /// unconditional DELETE at the top of the transaction empties the table on
    /// the next no-change poll. This test is what makes that removal loud.
    #[test]
    fn empty_starlink_batch_must_not_wipe_the_table() {
        let store = mem_store();
        store.insert_starlink_batch(&[sat(44713), sat(44714)]).unwrap();
        let before = store.get_starlink_all().unwrap();
        assert_eq!(before.as_array().unwrap().len(), 2, "setup failed");

        // A no-change poll: the fetch succeeded and produced no rows.
        store.insert_starlink_batch(&[]).unwrap();

        let after = store.get_starlink_all().unwrap();
        assert_eq!(
            after.as_array().unwrap().len(),
            2,
            "an empty batch wiped the starlink table; the guard in \
             insert_starlink_batch is load bearing and must stay"
        );
    }

    /// The replace still has to replace. A non-empty batch is a fresh snapshot
    /// and must not merge with what was there.
    #[test]
    fn a_non_empty_starlink_batch_still_replaces_everything() {
        let store = mem_store();
        store.insert_starlink_batch(&[sat(1), sat(2), sat(3)]).unwrap();
        store.insert_starlink_batch(&[sat(9)]).unwrap();

        let rows = store.get_starlink_all().unwrap();
        let rows = rows.as_array().unwrap();
        assert_eq!(rows.len(), 1, "the snapshot did not replace the old rows");
        assert_eq!(rows[0]["norad_id"].as_i64().unwrap(), 9);
    }
}
