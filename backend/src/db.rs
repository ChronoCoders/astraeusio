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
    /// A schema rewrite that did not verify. Startup fails rather than
    /// continuing on a half migrated table.
    #[error("migration failed: {0}")]
    Migration(String),
    /// An issue that did not carry every horizon. Nothing is written: a
    /// forecast history with some cycles missing a head is harder to notice
    /// than one missing it always.
    #[error("forecast horizons {got} are not the published set {want}, nothing was stored")]
    PartialForecast { got: String, want: String },
    /// A health snapshot for a component no reader enumerates. Declare it in
    /// db.rs rather than widening this.
    #[error("health snapshot for undeclared component {0}; add it to health_components()")]
    UndeclaredComponent(String),
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
    -- When the prediction was made, and how far ahead it reached. Keyed on both
    -- because a target time alone cannot tell two predictions apart: the 24 h
    -- head issued at 01:00 and the 12 h head issued at 13:00 name the same
    -- instant, and under the old key the second silently overwrote the first.
    issued_at      BIGINT NOT NULL,
    horizon_hours  BIGINT NOT NULL,
    -- issued_at + horizon_hours * 3600, stored rather than derived so a query
    -- can pair against it without recomputing the arithmetic in four places.
    ts             BIGINT NOT NULL,
    kp_e2          BIGINT NOT NULL,
    ci_lower_e2    BIGINT,
    ci_upper_e2    BIGINT,
    uncertainty_e4 BIGINT,
    -- Which checkpoint produced it, from the ml service's own /health. NULL for
    -- rows predating the fix that made the heads match their published lead, so
    -- a metric can ask about one model instead of averaging two.
    model_sha      TEXT,
    fetched_at     BIGINT NOT NULL,
    PRIMARY KEY (issued_at, horizon_hours)
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
    -- Nullable on purpose. A liveness component has no verdict to record: the
    -- row being present is the whole observation. See LIVENESS_ONLY.
    status    TEXT,
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
/// The lead times the ml service publishes, in hours, in the order it returns
/// them. One list, because three separate places used to assume the shape of
/// `forecast[]` and only the first element was ever stored.
pub const FORECAST_HORIZONS: [i64; 4] = [3, 6, 12, 24];

/// Pairs required before a forecast accuracy figure is published for a horizon.
///
/// Below this the number moves more under its own sampling noise than under any
/// change in the model, and a figure that swings on resampling is worse than an
/// empty cell: it invites a conclusion. At the measured residual spread of about
/// 0.7 Kp, the standard error of the mean absolute error is 0.7 / sqrt(n), which
/// is 0.31 at n = 5, 0.22 at n = 10 and 0.13 at n = 30. Thirty is where it drops
/// below the size of the differences anyone would act on, which for these
/// horizons is around 0.15 Kp.
///
/// At a 30 minute issue cadence that is about 15 hours of pairs for the 3 h head
/// and about 36 hours for the 24 h head, since a pair needs the target time to
/// have passed before it exists at all.
pub const MIN_PAIRS_FOR_METRICS: i64 = 30;


/// The two components judged by probing something rather than by reading a
/// table: the ml sidecar answers or it does not, and the Celestrak fetch has a
/// timestamp of its own.
pub const ML_COMPONENT: &str = "ml_forecast";
pub const CELESTRAK_COMPONENT: &str = "celestrak";
pub const PROBED: [&str; 2] = [ML_COMPONENT, CELESTRAK_COMPONENT];

/// Every component a health snapshot is ever written for.
///
/// The authoritative list, and the reason it exists is that there were three
/// hand-kept ones: the health handler inserting names, the uptime handler
/// holding `["backend_api", "ml_forecast", "database", "celestrak"]`, and
/// `poll_health` writing them. A component added to the writer and missed by a
/// reader simply had no history, which is how `noaa_alerts` showed an empty
/// strip from the day it was added.
///
/// Composed from the four declarations rather than typed out again, so adding a
/// series or a liveness feed reaches the readers without anyone remembering to
/// tell them. The set spans two writers: `poll_health` writes the series, the
/// probed pair and the liveness-only pair, while the alerts poller writes its
/// own `POLL_LIVENESS` verdict, which is why neither writer alone is the list.
pub fn health_components() -> Vec<&'static str> {
    SERIES_FRESHNESS
        .iter()
        .map(|s| s.component)
        .chain(POLL_LIVENESS.iter().map(|l| l.component))
        .chain(PROBED)
        .chain(LIVENESS_ONLY)
        .collect()
}

/// Components whose only honest claim is that the process was running.
///
/// `backend_api` used to record the literal `"operational"` written by the
/// backend itself, so the column held one value forever and the page published
/// a number that could not fall below 100. The column is gone for these: their
/// rows carry no status, and the row existing at a timestamp is the entire
/// observation.
///
/// That makes a full outage visible, because a backend that is down writes no
/// rows and the missing samples are counted against it. It does not make a
/// broken backend visible. A process that is running and answering every route
/// with a 500 writes exactly the rows a healthy one writes, and nothing inside
/// the box can tell the difference. The status page says so in a line rather
/// than leaving it to be inferred.
pub const BACKEND_COMPONENT: &str = "backend_api";
pub const DATABASE_COMPONENT: &str = "database";
pub const LIVENESS_ONLY: [&str; 2] = [BACKEND_COMPONENT, DATABASE_COMPONENT];

/// Named because the alerts poller writes a snapshot for it, and a name typed
/// at both the declaration and the writer is two names that happen to agree.
pub const ALERTS_COMPONENT: &str = "noaa_alerts";

pub const POLL_LIVENESS: [PollLiveness; 1] = [PollLiveness {
    // Polls every 300 s; six missed cycles is a fault and one is not.
    component: ALERTS_COMPONENT,
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

/// How long each table keeps rows, and the column that decides.
///
/// Per table rather than one number, because a five second poller and a five
/// minute one are not the same problem and neither is a table nothing reads
/// twice. The window is set by what actually queries the table, not by a
/// feeling about how much history is nice to have:
///
/// - `iss_position` is written every five seconds, 16,600 rows a day, and the
///   only read is `ORDER BY ts DESC LIMIT 1`. Thirty days rather than the seven
///   that reading alone justifies, because a position history is the obvious
///   thing a satellite tracking product grows into and thirty days of it is
///   cheap. If that turns out to be wrong it is one number.
/// - The NOAA series back charts and the thirty day report, so ninety days
///   covers the longest query with room.
/// - `health_snapshots` backs the ninety day uptime strip, so it keeps a
///   hundred: the strip must not thin out at its own left edge.
/// - `kp_forecast` and `alerts_anomaly` are user facing history and small.
/// - `kp_3h` is the model's input and eight rows a day. Two years costs
///   nothing and a retrain may want it.
///
/// `starlink` is deliberately absent. It holds a snapshot rather than history
/// and its problem was never retention; it is written in place now.
/// `neo`, `epic`, `apod` and `exoplanet` are absent too: hundreds of rows each,
/// nothing to reclaim, and `neo` is keyed on a forward date where "old" is not
/// a property of the row.
pub struct Retention {
    pub table: &'static str,
    pub time_column: &'static str,
    pub keep_days: i64,
}

pub const RETENTION: [Retention; 10] = [
    Retention { table: "iss_position", time_column: "ts", keep_days: 30 },
    Retention { table: "kp", time_column: "observed_at", keep_days: 90 },
    Retention { table: "solar_wind", time_column: "observed_at", keep_days: 90 },
    Retention { table: "imf", time_column: "observed_at", keep_days: 90 },
    Retention { table: "xray", time_column: "observed_at", keep_days: 90 },
    Retention { table: "health_snapshots", time_column: "ts", keep_days: 100 },
    Retention { table: "dst", time_column: "observed_at", keep_days: 365 },
    Retention { table: "kp_forecast", time_column: "ts", keep_days: 365 },
    Retention { table: "alerts_anomaly", time_column: "detected_at", keep_days: 365 },
    Retention { table: "kp_3h", time_column: "observed_at", keep_days: 730 },
];

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

/// Rekeys `kp_forecast` on `(issued_at, horizon_hours)` and relabels the rows
/// that predate `001cda9`.
const FORECAST_HORIZON_KEY_MIGRATION: &str = "2026-09-02-kp-forecast-horizon-key";

/// The moment `061a5d30fac5` began serving, as a fallback when the deployed
/// model file cannot be read.
///
/// Sourced, because a constant nobody can trace is how this needed correcting
/// in the first place. Three independent readings, all agreeing:
///
///   1. `ls --time-style=+%Y-%m-%dT%H:%M:%S /data/models/kp_lstm.pt` on the
///      volume: 2026-09-01T04:16:17. `deploy-model.sh` places the active model
///      with `cp`, which sets mtime to the moment of the deploy.
///   2. The first forecast issued afterwards, from `kp_forecast`:
///      2026-09-01 04:16:22, five seconds later, when the poller ran against
///      the restarted sidecar.
///   3. The last forecast issued before it: 2026-09-01 04:15. No row falls in
///      the gap, so any instant inside it classifies every row identically.
///
/// 1788236177 is reading 1 as a Unix second. Pinned by
/// `the_model_deploy_boundary_is_the_instant_it_is_documented_as`.
const MODEL_061A_DEPLOYED_AT: i64 = 1_788_236_177;

/// sha256 of `kp_lstm.pt` as served, from the ml `/health` field
/// `model_sha256` and from `sha256sum` of the file on the volume, which agree.
const MODEL_061A_SHA: &str = "061a5d30fac50c5f7e941730a37726c2bf02c008f72f484e8c01f143274760d1";

/// Relabels the rows that the horizon rekey filed as 6 h and that were in fact
/// 3 h forecasts from the model now serving.
const FORECAST_ERA_MIGRATION: &str = "2026-09-02-kp-forecast-model-era";

/// Marks the deploy verification accounts as verified.
const DEPLOY_ACCOUNTS_VERIFIED_MIGRATION: &str = "2026-09-02-verify-deploy-accounts";

/// The accounts `deploy.sh` signs in as to check authenticated routes.
///
/// Ours, on a domain we run, created by hand on 2026-08-10 through the ordinary
/// registration path, which is why they were never verified: nothing enforced
/// it and no mail was read. Once verification gates creating an API key they
/// would be locked out, and `deploy-verify-dev` holds the only live API key in
/// the system.
///
/// Marked verified by migration rather than exempted in code. An exemption is a
/// permanent branch that says "except these", which then has to be right
/// forever and is a place for a name to be added quietly. Setting the flag once
/// leaves them ordinary accounts afterwards, and the deploy checks keep working
/// without the gate having to know they exist.
const DEPLOY_ACCOUNTS: [&str; 2] = [
    "deploy-verify@astraeusio.com",
    "deploy-verify-dev@astraeusio.com",
];


/// When the active model was placed on the volume, read from the file itself.
///
/// The model lives beside the database on the shared volume, so the backend can
/// see it even though `MODEL_PATH` belongs to the ml service. Reading it beats
/// a pasted timestamp: the fact lives where the deploy put it, and a migration
/// that derives its boundary cannot disagree with the deploy that set it.
///
/// `None` when the file is absent, which is every in memory database and any
/// deployment whose volume has no model yet. The caller falls back to
/// `MODEL_061A_DEPLOYED_AT` and says which source it used.
fn model_deploy_time(db_path: &str) -> Option<i64> {
    let dir = std::path::Path::new(db_path).parent()?;
    let meta = std::fs::metadata(dir.join("models").join("kp_lstm.pt")).ok()?;
    let secs = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    i64::try_from(secs).ok()
}

/// Whether the corrected rows may be committed: exactly the rows that should
/// have moved moved, and nothing after the boundary carrying this model is left
/// disagreeing with its own lead.
///
/// The second term counts what is *wrong* rather than what is right, which is
/// the correction this needed. Counting the right ones compared 51 updated
/// against 52 correct and refused: the extra row was a forecast the running
/// system had already written correctly, so the gate failed on evidence that
/// the table was in better shape than it expected. A gate that fires when
/// nothing is wrong is worse than no gate, because the next person turns it off.
fn era_fix_is_verified(expected: i64, updated: i64, inconsistent: i64) -> bool {
    updated == expected && inconsistent == 0
}


/// Whether a rebuilt `kp_forecast` may replace the original.
///
/// Named and separate because with today's copy statement it cannot fail:
/// `INSERT ... SELECT` copies every row or none, and `ts` is computed from
/// `issued_at` in the same expression that sets it, so the two cannot disagree.
/// That is the point. The gate is not defending against DuckDB, it is defending
/// against the next edit to that statement, where a `WHERE` or a join would
/// drop rows silently and the only forecast history that exists would be gone
/// with the table it came from. `rebuild-db.sh` checks the same two things for
/// the same reason and it has never failed either.
fn rekey_is_verified(before: i64, copied: i64, inconsistent: i64) -> bool {
    copied == before && inconsistent == 0
}
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
            "ALTER TABLE health_snapshots ALTER COLUMN status DROP NOT NULL",
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

        // Rekey kp_forecast and relabel the rows that predate the horizon fix.
        //
        // Detected from the schema rather than from schema_migrations, because a
        // fresh database already has the new shape and must not run the copy.
        let needs_forecast_rekey: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM duckdb_columns()
                 WHERE table_name = 'kp_forecast' AND column_name = 'horizon_hours'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(1);
        if needs_forecast_rekey == 0 {
            let before: i64 =
                conn.query_row("SELECT COUNT(*) FROM kp_forecast", [], |row| row.get(0))?;
            let started = std::time::Instant::now();

            // Every existing row is a 3 h forecast by its label and a 6 h
            // forecast in fact. Before 001cda9 the training loop paired a window
            // ending at index i with the target at i + seq_len + p, one period
            // beyond the lead each head was published as, so the head sold as
            // 3 h was predicting 6 h. The value is real and the label was wrong,
            // so the label moves and `ts` moves with it. `model_sha` stays NULL,
            // which is what keeps these out of any figure describing the model
            // running now.
            // The copy is fallible for a reason that exists in the data: the
            // old key was the target time, so two rows could share a
            // `fetched_at` and collide on the new one. Production has none, and
            // a table left half built would fail every restart afterwards on
            // CREATE TABLE, so the temporary table goes whatever happens.
            let copy = conn.execute_batch(
                "CREATE TABLE kp_forecast_new (
                     issued_at      BIGINT NOT NULL,
                     horizon_hours  BIGINT NOT NULL,
                     ts             BIGINT NOT NULL,
                     kp_e2          BIGINT NOT NULL,
                     ci_lower_e2    BIGINT,
                     ci_upper_e2    BIGINT,
                     uncertainty_e4 BIGINT,
                     model_sha      TEXT,
                     fetched_at     BIGINT NOT NULL,
                     PRIMARY KEY (issued_at, horizon_hours)
                 );
                 INSERT INTO kp_forecast_new
                     SELECT fetched_at, 6, fetched_at + 6 * 3600, kp_e2,
                            ci_lower_e2, ci_upper_e2, uncertainty_e4, NULL, fetched_at
                     FROM kp_forecast;",
            );
            if let Err(e) = copy {
                let _ = conn.execute_batch("DROP TABLE IF EXISTS kp_forecast_new");
                error!(before, "kp_forecast rekey could not copy: {e}");
                return Err(DbError::Migration(format!(
                    "kp_forecast rekey could not copy {before} rows ({e}); the original                      table is untouched"
                )));
            }

            // Verify before anything is dropped, the way rebuild-db.sh does.
            // The old table is still the only copy of this history at this
            // point, and a rewrite that lost half of it would be unrecoverable
            // and silent. Two checks: every row arrived, and every row that
            // arrived is internally consistent.
            let copied: i64 =
                conn.query_row("SELECT COUNT(*) FROM kp_forecast_new", [], |row| row.get(0))?;
            let inconsistent: i64 = conn.query_row(
                "SELECT COUNT(*) FROM kp_forecast_new
                 WHERE ts != issued_at + horizon_hours * 3600",
                [],
                |row| row.get(0),
            )?;
            if !rekey_is_verified(before, copied, inconsistent) {
                // Nothing has moved. Drop the copy, leave the original alone,
                // and refuse to start rather than serve queries written for a
                // schema this database does not have.
                let _ = conn.execute_batch("DROP TABLE IF EXISTS kp_forecast_new");
                error!(
                    before,
                    copied, inconsistent, "kp_forecast rekey did not verify, nothing was moved"
                );
                return Err(DbError::Migration(format!(
                    "kp_forecast rekey copied {copied} of {before} rows with {inconsistent}                      inconsistent; the original table is untouched"
                )));
            }

            conn.execute_batch(
                "DROP TABLE kp_forecast;
                 ALTER TABLE kp_forecast_new RENAME TO kp_forecast;",
            )?;
            conn.execute(
                "INSERT INTO schema_migrations (id, applied_at) VALUES (?, ?)",
                params![FORECAST_HORIZON_KEY_MIGRATION, now()],
            )?;
            info!(
                rows = copied,
                ms = started.elapsed().as_millis() as u64,
                "rekeyed kp_forecast on (issued_at, horizon_hours) and relabelled the pre-001cda9 rows as 6 h"
            );
        }

        // The horizon rekey relabelled every pre-existing row as a 6 h forecast
        // from an unidentified model. That is right for 1305 of them and wrong
        // for 51: those were issued after `061a5d30fac5` was deployed, by the
        // checkpoint that fixed the off by one, so they are true 3 h forecasts
        // from the model serving now. Left alone they are mislabelled in the
        // opposite direction and excluded from the only metrics they could
        // populate.
        let era_fix_applied: i64 = conn.query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE id = ?",
            params![FORECAST_ERA_MIGRATION],
            |row| row.get(0),
        )?;
        if era_fix_applied == 0 {
            let (boundary, source) = match model_deploy_time(path) {
                Some(t) => (t, "the deployed model file"),
                None => (MODEL_061A_DEPLOYED_AT, "the recorded deploy time"),
            };

            // Counted before the update so the gate has something independent
            // to check the result against.
            let expected: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM kp_forecast WHERE issued_at >= ? AND model_sha IS NULL",
                    params![boundary],
                    |row| row.get(0),
                )
                .unwrap_or(0);

            if expected > 0 {
                // An explicit transaction, because DuckDB autocommits a bare
                // statement. The first version of this called ROLLBACK with no
                // transaction open, so a failed gate returned an error saying
                // "the table is unchanged" while the UPDATE it had just run
                // stayed applied. The message was false and the safety was not
                // there. Now the work happens between BEGIN and COMMIT, and the
                // failure path really does put it back.
                conn.execute_batch("BEGIN")?;
                let updated = conn.execute(
                    "UPDATE kp_forecast \
                        SET horizon_hours = 3, \
                            ts            = issued_at + 3 * 3600, \
                            model_sha     = ? \
                      WHERE issued_at >= ? AND model_sha IS NULL",
                    params![MODEL_061A_SHA, boundary],
                )? as i64;

                // What must be true after the boundary, stated as the thing
                // that would be wrong: no row left without a model, and no
                // row whose target disagrees with its own lead.
                //
                // Its own lead, not 3 h. An earlier version asserted every
                // row here was a 3 h forecast, which is true of the rows this
                // fixes and false of everything the poller writes now, so a
                // correct issue's 6, 12 and 24 h rows read as damage.
                let inconsistent: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM kp_forecast \n                      WHERE issued_at >= ? \n                        AND (model_sha IS NULL OR ts != issued_at + horizon_hours * 3600)",
                    params![boundary],
                    |row| row.get(0),
                )?;

                if !era_fix_is_verified(expected, updated, inconsistent) {
                    conn.execute_batch("ROLLBACK")?;
                    error!(
                        expected,
                        updated, inconsistent, boundary, "kp_forecast era fix did not verify"
                    );
                    return Err(DbError::Migration(format!(
                        "kp_forecast era fix updated {updated} of {expected} rows and left \
                         {inconsistent} disagreeing with their lead; the change was rolled back"
                    )));
                }
                conn.execute_batch("COMMIT")?;
                info!(
                    rows = updated,
                    boundary,
                    source,
                    "relabelled the forecasts issued by the current model as 3 h"
                );
            }

            conn.execute(
                "INSERT INTO schema_migrations (id, applied_at) VALUES (?, ?)",
                params![FORECAST_ERA_MIGRATION, now()],
            )?;
        }

        // The deploy accounts, marked verified once so the gate can stay
        // ignorant of them. Guarded by the migration record, so an address
        // deliberately un-verified later is not re-verified on the next restart.
        let deploy_verified_applied: i64 = conn.query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE id = ?",
            params![DEPLOY_ACCOUNTS_VERIFIED_MIGRATION],
            |row| row.get(0),
        )?;
        if deploy_verified_applied == 0 {
            let mut updated = 0i64;
            for email in DEPLOY_ACCOUNTS {
                updated += conn.execute(
                    "UPDATE users SET email_verified = TRUE WHERE email = ?",
                    params![email],
                )? as i64;
            }

            // Every named account that exists is now verified. Counted rather
            // than assumed: an address that is present and still unverified
            // means the update did not do what it says, and the deploy checks
            // would fail later for a reason nothing here recorded.
            let present: i64 = conn.query_row(
                "SELECT COUNT(*) FROM users WHERE email IN (?, ?)",
                params![DEPLOY_ACCOUNTS[0], DEPLOY_ACCOUNTS[1]],
                |row| row.get(0),
            )?;
            let unverified: i64 = conn.query_row(
                "SELECT COUNT(*) FROM users WHERE email IN (?, ?) AND email_verified IS NOT TRUE",
                params![DEPLOY_ACCOUNTS[0], DEPLOY_ACCOUNTS[1]],
                |row| row.get(0),
            )?;
            if unverified != 0 {
                return Err(DbError::Migration(format!(
                    "{unverified} of {present} deploy accounts are still unverified after the \
                     update; refusing to start rather than gate the deploy checks out"
                )));
            }

            conn.execute(
                "INSERT INTO schema_migrations (id, applied_at) VALUES (?, ?)",
                params![DEPLOY_ACCOUNTS_VERIFIED_MIGRATION, now()],
            )?;
            // Zero is the ordinary case on a fresh database, which has neither.
            info!(updated, present, "marked the deploy accounts verified");
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

/// One horizon of one forecast, as the ml service returned it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForecastPoint {
    pub horizon_hours: i64,
    pub kp_e2: i64,
    pub ci_lower_e2: Option<i64>,
    pub ci_upper_e2: Option<i64>,
    pub uncertainty_e4: Option<i64>,
}

impl ForecastPoint {
    /// Reads the ml service's `/predict` body into one point per horizon.
    ///
    /// Both write sites go through here, so the shape of `forecast[]` is
    /// understood in one place rather than in two that can drift. It returns
    /// nothing at all unless every published horizon is present with a usable
    /// prediction: a caller cannot store three of four by accident, because
    /// there is no value it can hold that represents three of four.
    pub fn from_predict_payload(
        payload: &serde_json::Value,
    ) -> Result<(Vec<ForecastPoint>, Option<String>), DbError> {
        let e2 = |v: Option<f64>| v.map(|x| (x * 100.0).round() as i64);
        let entries = payload.get("forecast").and_then(|v| v.as_array());
        let mut points = Vec::with_capacity(FORECAST_HORIZONS.len());
        for want in FORECAST_HORIZONS {
            let found = entries.and_then(|list| {
                list.iter().find(|e| {
                    e.get("horizon_hours").and_then(|v| v.as_i64()) == Some(want)
                })
            });
            let kp = found
                .and_then(|e| e.get("predicted_kp"))
                .and_then(|v| v.as_f64());
            let Some(kp) = kp else { continue };
            points.push(ForecastPoint {
                horizon_hours: want,
                kp_e2: (kp * 100.0).round() as i64,
                ci_lower_e2: e2(found.and_then(|e| e.get("ci_lower")).and_then(|v| v.as_f64())),
                ci_upper_e2: e2(found.and_then(|e| e.get("ci_upper")).and_then(|v| v.as_f64())),
                uncertainty_e4: found
                    .and_then(|e| e.get("uncertainty"))
                    .and_then(|v| v.as_f64())
                    .map(|x| (x * 10_000.0).round() as i64),
            });
        }
        if points.len() != FORECAST_HORIZONS.len() {
            let got: Vec<i64> = points.iter().map(|p| p.horizon_hours).collect();
            return Err(DbError::PartialForecast {
                got: format!("{got:?}"),
                want: format!("{:?}", FORECAST_HORIZONS),
            });
        }
        let sha = payload
            .get("model_sha256")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        Ok((points, sha))
    }
}

impl Store {
    /// Stores one issue: every horizon, or none of them.
    ///
    /// Four rows per cycle where there was one is four chances for a partial
    /// write, and a forecast history missing its 12 h rows on some cycles and
    /// not others is worse than one missing them always, because the gap is
    /// invisible in the aggregate. So the horizons arrive together in one call,
    /// go in under one transaction, and a set that is not exactly
    /// `FORECAST_HORIZONS` is refused before any of it is written.
    pub fn insert_kp_forecast(
        &self,
        issued_at: i64,
        model_sha: Option<&str>,
        points: &[ForecastPoint],
    ) -> Result<(), DbError> {
        let mut seen: Vec<i64> = points.iter().map(|p| p.horizon_hours).collect();
        seen.sort_unstable();
        let mut want = FORECAST_HORIZONS.to_vec();
        want.sort_unstable();
        if seen != want {
            return Err(DbError::PartialForecast {
                got: format!("{seen:?}"),
                want: format!("{want:?}"),
            });
        }

        // One transaction, the same shape as insert_kp_batch. Either the whole
        // issue is in the table or none of it is, so no reader can see two of
        // four horizons mid write.
        self.begin()?;
        let result = (|| {
            let mut stmt = self.conn.prepare(
                "INSERT INTO kp_forecast \
                     (issued_at, horizon_hours, ts, kp_e2, ci_lower_e2, ci_upper_e2, \
                      uncertainty_e4, model_sha, fetched_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) \
                 ON CONFLICT (issued_at, horizon_hours) DO UPDATE SET \
                     ts             = excluded.ts, \
                     kp_e2          = excluded.kp_e2, \
                     ci_lower_e2    = excluded.ci_lower_e2, \
                     ci_upper_e2    = excluded.ci_upper_e2, \
                     uncertainty_e4 = excluded.uncertainty_e4, \
                     model_sha      = excluded.model_sha, \
                     fetched_at     = excluded.fetched_at",
            )?;
            let written = now();
            for p in points {
                stmt.execute(params![
                    issued_at,
                    p.horizon_hours,
                    issued_at + p.horizon_hours * 3600,
                    p.kp_e2,
                    p.ci_lower_e2,
                    p.ci_upper_e2,
                    p.uncertainty_e4,
                    model_sha,
                    written,
                ])?;
            }
            Ok::<(), DbError>(())
        })();
        match result {
            Ok(()) => {
                self.commit()?;
                Ok(())
            }
            Err(e) => {
                self.rollback();
                Err(e)
            }
        }
    }

    /// Returns paired predicted/actual Kp rows for one horizon.
    /// Pairs each forecast `ts` with the closest `kp_3h` actual within ±90 minutes.
    pub fn get_forecast_history(
        &self,
        since: i64,
        horizon_hours: i64,
    ) -> Result<serde_json::Value, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT f.ts, f.issued_at, f.kp_e2, f.ci_lower_e2, f.ci_upper_e2, \
                    ( \
                      SELECT k.kp_e2 FROM kp_3h k \
                      WHERE abs(epoch(k.time_tag::TIMESTAMP) - f.ts) < 5400 \
                      ORDER BY abs(epoch(k.time_tag::TIMESTAMP) - f.ts) ASC \
                      LIMIT 1 \
                    ) AS actual_e2 \
             FROM kp_forecast f \
             WHERE f.ts > ? AND f.horizon_hours = ? \
             ORDER BY f.ts ASC",
        )?;
        let rows = stmt
            .query_map(params![since, horizon_hours], |row| {
                let ts: i64 = row.get(0)?;
                let issued_at: i64 = row.get(1)?;
                let kp_e2: i64 = row.get(2)?;
                let ci_l: Option<i64> = row.get(3)?;
                let ci_u: Option<i64> = row.get(4)?;
                let actual: Option<i64> = row.get(5)?;
                Ok(serde_json::json!({
                    "ts":            ts,
                    "issued_at":     issued_at,
                    "horizon_hours": horizon_hours,
                    "predicted_kp":  kp_e2 as f64 / 100.0,
                    "ci_lower":      ci_l.map(|v| v as f64 / 100.0),
                    "ci_upper":      ci_u.map(|v| v as f64 / 100.0),
                    "actual_kp":     actual.map(|v| v as f64 / 100.0),
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(serde_json::Value::Array(rows))
    }

    /// Forecast-vs-actual metrics per horizon over the last `since` seconds.
    ///
    /// Rows with no `model_sha` are excluded. Those are the pre-`001cda9`
    /// predictions, kept because they are real 6 h forecasts honestly relabelled
    /// and discarded by nobody, but produced by a checkpoint that is not the one
    /// serving. Averaging two models into one accuracy figure would answer a
    /// question nobody asked.
    ///
    /// A horizon with fewer than `MIN_PAIRS_FOR_METRICS` pairs reports its count
    /// and nothing else, so the page can leave the row empty rather than print a
    /// figure that moves under its own noise.
    pub fn get_forecast_metrics(&self, since: i64) -> Result<serde_json::Value, DbError> {
        let mut stmt = self.conn.prepare(
            "WITH paired AS ( \
               SELECT f.horizon_hours AS h, \
                      f.kp_e2 AS pred, \
                      ( \
                        SELECT k.kp_e2 FROM kp_3h k \
                        WHERE abs(epoch(k.time_tag::TIMESTAMP) - f.ts) < 5400 \
                        ORDER BY abs(epoch(k.time_tag::TIMESTAMP) - f.ts) ASC \
                        LIMIT 1 \
                      ) AS actual, \
                      f.uncertainty_e4 AS unc \
               FROM kp_forecast f \
               WHERE f.ts > ? AND f.model_sha IS NOT NULL \
             ) \
             SELECT h, \
               COUNT(*) FILTER (WHERE actual IS NOT NULL) AS n, \
               AVG(ABS(pred - actual)) FILTER (WHERE actual IS NOT NULL) AS mae_e2, \
               SQRT(AVG((pred - actual) * (pred - actual)) FILTER (WHERE actual IS NOT NULL)) AS rmse_e2, \
               COUNT(*) FILTER (WHERE actual >= 500)                                AS n_storms, \
               COUNT(*) FILTER (WHERE actual >= 500 AND pred >= 500)                AS n_storms_caught, \
               COUNT(*) FILTER (WHERE pred >= 500 AND (actual IS NOT NULL AND actual < 500)) AS n_false_pos, \
               AVG(unc) FILTER (WHERE unc IS NOT NULL) AS mean_unc_e4 \
             FROM paired GROUP BY h",
        )?;
        let measured = stmt
            .query_map([since], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                    row.get::<_, Option<f64>>(2)?,
                    row.get::<_, Option<f64>>(3)?,
                    row.get::<_, Option<i64>>(4)?.unwrap_or(0),
                    row.get::<_, Option<i64>>(5)?.unwrap_or(0),
                    row.get::<_, Option<i64>>(6)?.unwrap_or(0),
                    row.get::<_, Option<f64>>(7)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // Enumerated from FORECAST_HORIZONS, not from what the query returned.
        // A horizon that has never been stored has to appear as an empty row,
        // otherwise a head that stopped being written would vanish from the page
        // instead of showing that it has no pairs.
        let horizons: Vec<serde_json::Value> = FORECAST_HORIZONS
            .iter()
            .map(|h| {
                let found = measured.iter().find(|m| m.0 == *h).copied();
                let n = found.map(|m| m.1).unwrap_or(0);
                if n < MIN_PAIRS_FOR_METRICS {
                    return serde_json::json!({
                        "horizon_hours": h,
                        "n_samples":     n,
                        "min_samples":   MIN_PAIRS_FOR_METRICS,
                        "sufficient":    false,
                    });
                }
                let (_, _, mae, rmse, n_storms, n_caught, n_false, unc) =
                    found.unwrap_or((*h, 0, None, None, 0, 0, 0, None));
                serde_json::json!({
                    "horizon_hours": h,
                    "n_samples":     n,
                    "min_samples":   MIN_PAIRS_FOR_METRICS,
                    "sufficient":    true,
                    "mae":           mae.map(|v| v / 100.0),
                    "rmse":          rmse.map(|v| v / 100.0),
                    "n_storms":      n_storms,
                    "n_caught":      n_caught,
                    "n_false_pos":   n_false,
                    "hit_rate":      if n_storms > 0 { Some(n_caught as f64 / n_storms as f64) } else { None },
                    "mean_unc":      unc.map(|v| v / 10_000.0),
                })
            })
            .collect();

        Ok(serde_json::json!({ "horizons": horizons }))
    }

    /// The most recent prediction at one horizon, as `(target_ts, kp_e2)`.
    pub fn get_kp_forecast_latest(&self, horizon_hours: i64) -> Result<Option<(i64, i64)>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT ts, kp_e2 FROM kp_forecast WHERE horizon_hours = ? \
             ORDER BY issued_at DESC LIMIT 1",
        )?;
        let mut rows = stmt.query([horizon_hours])?;
        if let Some(row) = rows.next()? {
            Ok(Some((row.get(0)?, row.get(1)?)))
        } else {
            Ok(None)
        }
    }

    /// The highest predicted Kp at one horizon among forecasts issued since
    /// `since`, as `(target_ts, kp_e2)`.
    pub fn get_kp_forecast_max_recent(
        &self,
        since: i64,
        horizon_hours: i64,
    ) -> Result<Option<(i64, i64)>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT ts, kp_e2 FROM kp_forecast \
             WHERE issued_at > ? AND horizon_hours = ? \
             ORDER BY kp_e2 DESC LIMIT 1",
        )?;
        let mut rows = stmt.query(params![since, horizon_hours])?;
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
            // Named for the window it counts. This is today forward by the
            // range, while every other figure here describes the range just
            // past, and calling it "asteroid_approaches" inside a summary of
            // the last 30 days invited exactly one reading, the wrong one.
            "upcoming_approaches": asteroid_count,
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
    /// Marks the address proven, and reports whether this call is what proved
    /// it.
    ///
    /// `AND email_verified IS NOT TRUE` is what makes the row count mean
    /// something. Without it an already verified account still reports one row
    /// changed, so the caller cannot tell a first use of a link from a replay:
    /// the verification token stayed usable for its whole life and sent a
    /// welcome mail on every use, which made a captured link one mail per
    /// request. Putting the test inside the statement rather than reading the
    /// row first also means two uses of the same link racing cannot both win.
    pub fn set_email_verified(&self, email: &str) -> Result<bool, DbError> {
        let changed = self.conn.execute(
            "UPDATE users SET email_verified = TRUE              WHERE email = ? AND email_verified IS NOT TRUE",
            params![email],
        )?;
        Ok(changed == 1)
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

    /// Deletes rows past their table's retention window.
    ///
    /// Reports what it removed rather than returning nothing, so the log line
    /// says which table and how many and a window set too tight is visible the
    /// first time it runs rather than after somebody notices a chart is short.
    ///
    /// Deleting does not shrink the file. DuckDB frees the space for reuse
    /// inside the database, so this bounds growth; `rebuild-db.sh` is what
    /// returns space to the disk, and it is deliberately a separate manual step
    /// because it has to stop the backend.
    pub fn purge_expired(&self) -> Result<Vec<(&'static str, usize)>, DbError> {
        let now = now();
        let mut purged = Vec::new();
        for rule in RETENTION.iter() {
            let cutoff = now - rule.keep_days * 86_400;
            // `table` and `time_column` come from RETENTION, never from a
            // request, which is why they can be formatted into the statement.
            let sql = format!(
                "DELETE FROM {} WHERE {} < ?",
                rule.table, rule.time_column
            );
            let removed = self.conn.execute(&sql, params![cutoff])?;
            if removed > 0 {
                purged.push((rule.table, removed));
            }
        }
        Ok(purged)
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
        let batch_at = now();
        let result = (|| {
            // Upsert rather than DELETE + INSERT, which is what this was until
            // 2026-09-01 and what made this table 83 percent of the database:
            // 3688 of the 4450 blocks in the file, holding 10725 rows worth
            // about 1.7 MB. A full replace appends new row groups every cycle
            // and leaves the old ones behind, and nothing reclaims them.
            //
            // Measured from a compacted copy, twelve cycles each. DELETE +
            // INSERT grew the file 1.10 MB per cycle and took the table from 6
            // blocks to 55; this upsert grew it by nothing and left 4 blocks. At
            // roughly twelve write cycles a day that is 13 MB a day, against the
            // 9.6 MB a day the database was actually growing, so this one write
            // was essentially all of it.
            let mut stmt = self.conn.prepare(
                "INSERT INTO starlink (norad_id, name, tle_line1, tle_line2, fetched_at)
                 VALUES (?, ?, ?, ?, ?)
                 ON CONFLICT (norad_id) DO UPDATE SET
                     name = excluded.name,
                     tle_line1 = excluded.tle_line1,
                     tle_line2 = excluded.tle_line2,
                     fetched_at = excluded.fetched_at",
            )?;
            for sat in sats {
                stmt.execute(params![
                    sat.norad_id,
                    sat.name,
                    sat.tle_line1,
                    sat.tle_line2,
                    batch_at
                ])?;
            }
            // Satellites that left the constellation.
            //
            // Membership, not timestamps. Marking rows with `batch_at` and
            // deleting anything older reads well and is wrong: `fetched_at` is
            // whole seconds, so two batches inside one second carry the same
            // mark and the departures survive. The test caught it immediately.
            // A temp table is exact whatever the clock resolution, and being
            // temporary it costs the main file nothing.
            self.conn
                .execute_batch("CREATE OR REPLACE TEMP TABLE starlink_seen (norad_id INTEGER)")?;
            {
                let mut seen = self
                    .conn
                    .prepare("INSERT INTO starlink_seen (norad_id) VALUES (?)")?;
                for sat in sats {
                    seen.execute(params![sat.norad_id])?;
                }
            }
            self.conn.execute_batch(
                "DELETE FROM starlink                  WHERE norad_id NOT IN (SELECT norad_id FROM starlink_seen)",
            )?;
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

    /// `status` is `None` for the components in `LIVENESS_ONLY`, which have no
    /// verdict to record. The row is the observation.
    ///
    /// An undeclared component is refused rather than stored. Reading the
    /// writers' source catches a name typed at the `component:` field and not a
    /// name typed in the loop above it, which mutation testing demonstrated on
    /// 2026-09-02 by replacing `LIVENESS_ONLY` with a literal list and breaking
    /// nothing. Refusing the write closes that: a component the readers cannot
    /// enumerate cannot accumulate history they will never show.
    pub fn insert_health_snapshot(
        &self,
        component: &str,
        ts: i64,
        status: Option<&str>,
    ) -> Result<(), DbError> {
        if !health_components().contains(&component) {
            return Err(DbError::UndeclaredComponent(component.to_string()));
        }
        self.conn.execute(
            "INSERT OR IGNORE INTO health_snapshots (component, ts, status) VALUES (?, ?, ?)",
            params![component, ts, status],
        )?;
        Ok(())
    }

    /// Returns per-component daily counts over the last `days` days.
    /// Each row: (component, utc_day, samples_present, operational_samples).
    ///
    /// `utc_day` is `ts / 86400`, a calendar day since the epoch. It used to be
    /// `(now - ts) / 86400`, a rolling offset from request time, so the same
    /// historical sample landed in a different cell depending on the hour the
    /// page was loaded while the frontend labelled the cells as days.
    ///
    /// The counts are raw. Turning them into a percentage needs the number of
    /// samples that *should* be there, which is why this no longer computes one:
    /// dividing operational rows by rows present made absence invisible, and
    /// absence is the only evidence an outage leaves.
    pub fn uptime_by_day(&self, days: i64) -> Result<Vec<(String, i64, i64, i64)>, DbError> {
        let since = now() - days * 86_400;
        let mut stmt = self.conn.prepare(
            // `//` and not `/`. DuckDB's `/` is float division, so
             // `CAST(ts / 86400 AS BIGINT)` rounded, and every sample after
             // midday was filed under the following day. The rolling bucket
             // this replaced had the same rounding on top of its own problem.
             "SELECT component,
                    CAST(ts // 86400 AS BIGINT) AS utc_day,
                    COUNT(*),
                    SUM(CASE WHEN status = 'operational' THEN 1 ELSE 0 END)
             FROM health_snapshots
             WHERE ts >= ?
             GROUP BY component, utc_day
             ORDER BY component, utc_day",
        )?;
        let rows = stmt
            .query_map(params![since], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, Option<i64>>(3)?.unwrap_or(0),
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// The first timestamp ever recorded for each component.
    ///
    /// Expectation starts here and not before. `2623cf6` decided that a day
    /// with no samples is unrecorded rather than an outage, because six NOAA
    /// components read as three months of downtime on the day they were added.
    /// That decision survives an expected-count denominator exactly by this:
    /// before a component's first sample nothing was expected, so there is no
    /// gap to hold against it. After it, a missing sample is a missing sample.
    pub fn health_first_sample(&self) -> Result<Vec<(String, i64)>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT component, MIN(ts) FROM health_snapshots GROUP BY component")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
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

    /// Retention removes what is past its window and nothing else, and every
    /// rule names a table and column that exist.
    ///
    /// The second half matters more than it looks: a renamed column would make
    /// the purge fail at runtime on a table nobody is watching, and the growth
    /// it was meant to bound would return silently (AUD-023).
    #[test]
    fn retention_removes_only_what_is_past_its_window() {
        let store = mem_store();
        let now = now();

        for rule in RETENTION.iter() {
            let sql = format!(
                "SELECT {} FROM {} LIMIT 0",
                rule.time_column, rule.table
            );
            assert!(
                store.conn.prepare(&sql).is_ok(),
                "{} has no column {}",
                rule.table,
                rule.time_column
            );
        }

        // iss_position keeps 30 days. One row inside the window, one outside.
        let insert = |ts: i64, id: i64| {
            store
                .conn
                .execute(
                    "INSERT INTO iss_position (ts, lat_e6, lon_e6, altitude_m, velocity_m_h)                      VALUES (?, ?, 0, 0, 0)",
                    params![ts, id],
                )
                .expect("insert");
        };
        insert(now - 29 * 86_400, 1);
        insert(now - 31 * 86_400, 2);
        // health_snapshots keeps 100, so a 31 day old row must survive there.
        store
            .insert_health_snapshot("backend_api", now - 31 * 86_400, Some("operational"))
            .expect("snapshot");

        let purged = store.purge_expired().expect("purge");
        let removed: std::collections::HashMap<_, _> = purged.into_iter().collect();
        assert_eq!(removed.get("iss_position"), Some(&1), "one row was past 30 days");
        assert!(
            !removed.contains_key("health_snapshots"),
            "31 days is inside the 100 day window"
        );

        let left: i64 = store
            .conn
            .query_row("SELECT count(*) FROM iss_position", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 1, "the row inside the window must stay");

        // Running again removes nothing: the purge is idempotent.
        assert!(store.purge_expired().expect("purge").is_empty());
    }

    /// A satellite that stays in the constellation keeps its row and gets its
    /// TLE updated in place, rather than being deleted and reinserted.
    ///
    /// The distinction is invisible in the results and is the whole point:
    /// DELETE plus INSERT appended new row groups every cycle and left the old
    /// ones, which grew this table to 83 percent of the database while holding
    /// 1.7 MB of live data (AUD-023).
    #[test]
    fn a_returning_satellite_is_updated_rather_than_replaced() {
        let store = mem_store();
        store.insert_starlink_batch(&[sat(1), sat(2)]).unwrap();

        // Same satellite, new elements, plus one that has joined.
        let moved = StarlinkSat {
            norad_id: 1,
            name: "STARLINK-1".to_owned(),
            tle_line1: "1 00001U MOVED".to_owned(),
            tle_line2: "2 00001 MOVED".to_owned(),
        };
        store.insert_starlink_batch(&[moved, sat(3)]).unwrap();

        let rows = store.get_starlink_all().unwrap();
        let rows = rows.as_array().unwrap();
        let ids: Vec<i64> = rows.iter().map(|r| r["norad_id"].as_i64().unwrap()).collect();

        assert_eq!(ids, vec![1, 3], "2 left the constellation and must go");
        let first = rows.iter().find(|r| r["norad_id"] == 1).expect("1 is still here");
        assert_eq!(
            first["tle_line1"].as_str().unwrap(),
            "1 00001U MOVED",
            "a returning satellite must carry its new elements"
        );
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
            .insert_health_snapshot("noaa_alerts", now() - 60, Some("operational"))
            .unwrap();
        assert_eq!(status(&store), "operational");

        store
            .insert_health_snapshot("noaa_alerts", now() - 30, Some("degraded"))
            .unwrap();
        assert_eq!(status(&store), "degraded", "the newest verdict wins");

        // Six missed cycles. The last verdict was operational and it no longer
        // means anything.
        let store = mem_store();
        store
            .insert_health_snapshot("noaa_alerts", now() - 3_600, Some("operational"))
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

    /// A five second window in a WHERE clause is exactly the kind of constant
    /// that becomes wrong silently, so it is pinned to the instant it is
    /// documented as. If someone edits the number, this says so, and if the
    /// boundary genuinely moves the comment above it has to move with it.
    #[test]
    fn the_model_deploy_boundary_is_the_instant_it_is_documented_as() {
        let t = chrono::DateTime::from_timestamp(MODEL_061A_DEPLOYED_AT, 0).expect("a real time");
        assert_eq!(
            t.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            "2026-09-01T04:16:17Z",
            "the fallback boundary must be the deploy time the comment cites"
        );
        assert_eq!(MODEL_061A_SHA.len(), 64, "a sha256 is 64 hex characters");
        assert!(
            MODEL_061A_SHA.chars().all(|c| c.is_ascii_hexdigit()),
            "and nothing else"
        );
    }

    /// The boundary comes from the deploy that set it, not from a number typed
    /// beside it. `deploy-model.sh` places the active checkpoint with `cp`, so
    /// the file's mtime is the moment it began serving, and the backend can see
    /// it because the model sits beside the database on the shared volume.
    #[test]
    fn the_boundary_is_read_from_the_deployed_model_file() {
        let dir = std::env::temp_dir().join(format!("astraeus-boundary-{}", now()));
        std::fs::create_dir_all(dir.join("models")).expect("models dir");
        let db = dir.join("astraeus.duckdb");
        let model = dir.join("models").join("kp_lstm.pt");
        std::fs::write(&model, b"checkpoint").expect("write");

        let want = 1_788_236_177;
        filetime_set(&model, want);
        assert_eq!(
            model_deploy_time(db.to_str().unwrap()),
            Some(want),
            "the deploy time is the model file's own mtime"
        );

        // No model on the volume, which is every in memory database and any
        // deployment that has not had one placed yet.
        std::fs::remove_file(&model).expect("remove");
        assert_eq!(
            model_deploy_time(db.to_str().unwrap()),
            None,
            "an absent model yields nothing, and the caller falls back"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Sets a file's mtime to a Unix second, so a test can describe a deploy
    /// that happened at a known instant.
    fn filetime_set(path: &std::path::Path, secs: i64) {
        let f = std::fs::OpenOptions::new().write(true).open(path).expect("open");
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs as u64);
        f.set_modified(t).expect("set mtime");
    }

    /// The correction, on the shape the rekey actually left behind: rows from
    /// the old checkpoint keep their 6 h label and their absent model, rows
    /// issued after the deploy become the 3 h forecasts they always were.
    ///
    /// The gate is the same one the rekey uses. A run that moved the wrong
    /// number of rows, or left one whose `ts` disagrees with its lead, fails
    /// startup instead of serving a half corrected table.
    #[test]
    fn only_the_forecasts_issued_after_the_deploy_are_relabelled() {
        let boundary = MODEL_061A_DEPLOYED_AT;
        // Six before the deploy and four after, in the post rekey shape.
        let before: Vec<i64> = (1..=6).map(|i| boundary - i * 1800).collect();
        let after: Vec<i64> = (0..4).map(|i| boundary + 5 + i * 1800).collect();

        let path = std::env::temp_dir().join(format!("astraeus-era-{}.duckdb", now()));
        let _ = std::fs::remove_file(&path);
        {
            let conn = duckdb::Connection::open(&path).expect("open");
            conn.execute_batch(SCHEMA).expect("schema");
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS schema_migrations (
                     id TEXT NOT NULL PRIMARY KEY, applied_at BIGINT NOT NULL);",
            )
            .expect("migrations table");
            for id in [PURGE_FORECASTS_MIGRATION, FORECAST_HORIZON_KEY_MIGRATION] {
                conn.execute(
                    "INSERT INTO schema_migrations (id, applied_at) VALUES (?, ?)",
                    params![id, now()],
                )
                .expect("record");
            }
            // Exactly what the rekey leaves: everything 6 h, no model named.
            for issued in before.iter().chain(after.iter()) {
                conn.execute(
                    "INSERT INTO kp_forecast
                       (issued_at, horizon_hours, ts, kp_e2, model_sha, fetched_at)
                     VALUES (?, 6, ? + 6 * 3600, 300, NULL, ?)",
                    params![issued, issued, issued],
                )
                .expect("seed");
            }
        }

        let store = Store::open(path.to_str().unwrap()).expect("migrated open");

        let (threes, sixes, named, inconsistent): (i64, i64, i64, i64) = store
            .conn
            .query_row(
                "SELECT COUNT(*) FILTER (WHERE horizon_hours = 3), \
                        COUNT(*) FILTER (WHERE horizon_hours = 6), \
                        COUNT(*) FILTER (WHERE model_sha IS NOT NULL), \
                        COUNT(*) FILTER (WHERE ts != issued_at + horizon_hours * 3600) \
                 FROM kp_forecast",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .expect("counts");

        assert_eq!(threes, after.len() as i64, "only the post deploy rows moved");
        assert_eq!(sixes, before.len() as i64, "the old checkpoint's rows are untouched");
        assert_eq!(named, after.len() as i64, "and only the moved rows name a model");
        assert_eq!(inconsistent, 0, "every row's target matches its own lead");

        let sha: String = store
            .conn
            .query_row(
                "SELECT model_sha FROM kp_forecast WHERE horizon_hours = 3 LIMIT 1",
                [],
                |r| r.get(0),
            )
            .expect("sha");
        assert_eq!(sha, MODEL_061A_SHA, "the rows carry the checkpoint that made them");

        // Recorded, so a restart does not sweep a later era's rows into this one.
        let recorded: i64 = store
            .conn
            .query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE id = ?",
                params![FORECAST_ERA_MIGRATION],
                |r| r.get(0),
            )
            .expect("recorded");
        assert_eq!(recorded, 1);

        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    /// The migration reads the deploy time from the model file, not from the
    /// constant sitting next to it. Proved by making the two disagree: the file
    /// says two hours after the pinned value, and a row issued between them must
    /// stay untouched. If the migration fell back to the constant it would
    /// relabel that row, which is the silent version of getting the boundary
    /// wrong and is what needed correcting the first time.
    #[test]
    fn the_migration_takes_its_boundary_from_the_model_file() {
        let literal = MODEL_061A_DEPLOYED_AT;
        let from_file = literal + 7200;

        let dir = std::env::temp_dir().join(format!("astraeus-wire-{}", now()));
        std::fs::create_dir_all(dir.join("models")).expect("models dir");
        let model = dir.join("models").join("kp_lstm.pt");
        std::fs::write(&model, b"checkpoint").expect("write");
        filetime_set(&model, from_file);

        let path = dir.join("astraeus.duckdb");
        // Before the constant, between the two, and after the file's time.
        let rows = [literal - 1800, literal + 60, from_file + 60];
        {
            let conn = duckdb::Connection::open(&path).expect("open");
            conn.execute_batch(SCHEMA).expect("schema");
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS schema_migrations (
                     id TEXT NOT NULL PRIMARY KEY, applied_at BIGINT NOT NULL);",
            )
            .expect("migrations table");
            for id in [PURGE_FORECASTS_MIGRATION, FORECAST_HORIZON_KEY_MIGRATION] {
                conn.execute(
                    "INSERT INTO schema_migrations (id, applied_at) VALUES (?, ?)",
                    params![id, now()],
                )
                .expect("record");
            }
            for issued in rows {
                conn.execute(
                    "INSERT INTO kp_forecast
                       (issued_at, horizon_hours, ts, kp_e2, model_sha, fetched_at)
                     VALUES (?, 6, ? + 6 * 3600, 300, NULL, ?)",
                    params![issued, issued, issued],
                )
                .expect("seed");
            }
        }

        let store = Store::open(path.to_str().unwrap()).expect("migrated open");
        let corrected: Vec<i64> = {
            let mut stmt = store
                .conn
                .prepare("SELECT issued_at FROM kp_forecast WHERE horizon_hours = 3 ORDER BY issued_at")
                .expect("prepare");
            stmt.query_map([], |r| r.get::<_, i64>(0))
                .expect("query")
                .collect::<Result<Vec<_>, _>>()
                .expect("rows")
        };

        assert_eq!(
            corrected,
            vec![from_file + 60],
            "only the row issued after the model file's own mtime is relabelled; \
             the row an hour past the pinned constant proves the constant was not used"
        );

        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The shape production was actually in, which the first version of this gate
    /// refused: rows needing correction alongside an issue the running system
    /// had already written correctly, at every horizon, carrying the same model.
    ///
    /// That correct issue made the count of consistent rows exceed the count of
    /// updated rows, and the gate read the difference as damage. It failed the
    /// migration, the process exited, and because the UPDATE had run outside any
    /// transaction the change stayed applied while the error said the table was
    /// unchanged. Both halves are fixed here and this is what proves it: the
    /// migration completes, and the already correct rows are neither counted
    /// against it nor touched.
    #[test]
    fn rows_the_running_system_wrote_correctly_do_not_fail_the_correction() {
        let boundary = MODEL_061A_DEPLOYED_AT;
        let stale: Vec<i64> = (0..3).map(|i| boundary + 60 + i * 1800).collect();
        let already_correct = boundary + 9000;

        let path = std::env::temp_dir().join(format!("astraeus-regress-{}.duckdb", now()));
        let _ = std::fs::remove_file(&path);
        {
            let conn = duckdb::Connection::open(&path).expect("open");
            conn.execute_batch(SCHEMA).expect("schema");
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS schema_migrations (
                     id TEXT NOT NULL PRIMARY KEY, applied_at BIGINT NOT NULL);",
            )
            .expect("migrations table");
            for id in [PURGE_FORECASTS_MIGRATION, FORECAST_HORIZON_KEY_MIGRATION] {
                conn.execute(
                    "INSERT INTO schema_migrations (id, applied_at) VALUES (?, ?)",
                    params![id, now()],
                )
                .expect("record");
            }
            // Mislabelled by the rekey: 6 h, no model.
            for issued in &stale {
                conn.execute(
                    "INSERT INTO kp_forecast
                       (issued_at, horizon_hours, ts, kp_e2, model_sha, fetched_at)
                     VALUES (?, 6, ? + 6 * 3600, 300, NULL, ?)",
                    params![issued, issued, issued],
                )
                .expect("seed stale");
            }
            // One full issue written by the new code path: every horizon, right
            // target, model named. Exactly what the poller wrote at 03:10:10.
            for h in FORECAST_HORIZONS {
                conn.execute(
                    "INSERT INTO kp_forecast
                       (issued_at, horizon_hours, ts, kp_e2, model_sha, fetched_at)
                     VALUES (?, ?, ? + ? * 3600, 250, ?, ?)",
                    params![already_correct, h, already_correct, h, MODEL_061A_SHA, already_correct],
                )
                .expect("seed correct");
            }
        }

        let store = Store::open(path.to_str().unwrap()).expect("the migration must not refuse this");

        let (threes, named, inconsistent, total): (i64, i64, i64, i64) = store
            .conn
            .query_row(
                "SELECT COUNT(*) FILTER (WHERE horizon_hours = 3), \
                        COUNT(*) FILTER (WHERE model_sha IS NOT NULL), \
                        COUNT(*) FILTER (WHERE ts != issued_at + horizon_hours * 3600), \
                        COUNT(*) FROM kp_forecast",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .expect("counts");

        // The three stale rows plus the correct issue's own 3 h row.
        assert_eq!(threes, stale.len() as i64 + 1);
        assert_eq!(named, stale.len() as i64 + FORECAST_HORIZONS.len() as i64);
        assert_eq!(inconsistent, 0);
        assert_eq!(total, stale.len() as i64 + FORECAST_HORIZONS.len() as i64, "no row was added or lost");

        // The already correct issue kept all four of its horizons untouched.
        let kept: i64 = store
            .conn
            .query_row(
                "SELECT COUNT(*) FROM kp_forecast WHERE issued_at = ?",
                params![already_correct],
                |r| r.get(0),
            )
            .expect("kept");
        assert_eq!(
            kept,
            FORECAST_HORIZONS.len() as i64,
            "the correction must not collapse a correct issue onto one horizon"
        );

        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    /// The same rule the rekey applies, stated once and tested as a rule. With
    /// today's single UPDATE it cannot fail, which is the point: it is there for
    /// the next edit to that statement, not for DuckDB.
    #[test]
    fn a_corrected_table_commits_only_when_it_verifies() {
        assert!(era_fix_is_verified(51, 51, 0), "all moved, nothing left wrong");
        assert!(!era_fix_is_verified(51, 50, 0), "one row short does not commit");
        assert!(!era_fix_is_verified(51, 51, 1), "one row disagreeing with its lead does not commit");
        assert!(!era_fix_is_verified(51, 52, 0), "more than expected does not commit either");
    }

    /// The deploy accounts are marked verified once, and nothing else is.
    ///
    /// `deploy-verify-dev` holds the only live API key, and creating a key is
    /// one of the things verification now gates, so getting this wrong locks
    /// the deploy checks out of the system they check. An ordinary account in
    /// the same database must be untouched: this is a fix for two known rows,
    /// not a general amnesty.
    #[test]
    fn the_deploy_accounts_are_verified_and_no_one_else_is() {
        let path = std::env::temp_dir().join(format!("astraeus-depver-{}.duckdb", now()));
        let _ = std::fs::remove_file(&path);
        {
            let conn = duckdb::Connection::open(&path).expect("open");
            conn.execute_batch(SCHEMA).expect("schema");
            // No email_verified column yet: it arrives as an ALTER during
            // Store::open, defaulting to FALSE, which is the state these three
            // rows were really in.
            for email in DEPLOY_ACCOUNTS {
                conn.execute(
                    "INSERT INTO users (email, password_hash, created_at)
                     VALUES (?, 'hash', 0)",
                    params![email],
                )
                .expect("seed deploy account");
            }
            conn.execute(
                "INSERT INTO users (email, password_hash, created_at)
                 VALUES ('someone@example.com', 'hash', 0)",
                [],
            )
            .expect("seed ordinary account");
        }

        let store = Store::open(path.to_str().unwrap()).expect("migrated open");

        for email in DEPLOY_ACCOUNTS {
            let verified: bool = store
                .conn
                .query_row(
                    "SELECT email_verified FROM users WHERE email = ?",
                    params![email],
                    |r| r.get(0),
                )
                .expect("row");
            assert!(verified, "{email} must be verified or the deploy checks break");
        }

        let other: bool = store
            .conn
            .query_row(
                "SELECT email_verified FROM users WHERE email = 'someone@example.com'",
                [],
                |r| r.get(0),
            )
            .expect("row");
        assert!(!other, "an ordinary account must not be swept up");

        let recorded: i64 = store
            .conn
            .query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE id = ?",
                params![DEPLOY_ACCOUNTS_VERIFIED_MIGRATION],
                |r| r.get(0),
            )
            .expect("recorded");
        assert_eq!(recorded, 1, "recorded, so a later un-verify is not undone on restart");

        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    /// A database with neither account applies cleanly and records itself.
    /// Every fresh deployment is that case.
    #[test]
    fn the_deploy_account_migration_is_fine_with_neither_present() {
        let store = mem_store();
        let recorded: i64 = store
            .conn
            .query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE id = ?",
                params![DEPLOY_ACCOUNTS_VERIFIED_MIGRATION],
                |r| r.get(0),
            )
            .expect("recorded");
        assert_eq!(recorded, 1);
    }

    /// A component no reader enumerates cannot accumulate history.
    ///
    /// The source scan next door catches a name typed at the `component:`
    /// field. It does not catch a name typed in the loop above it, which a
    /// mutation demonstrated by swapping `LIVENESS_ONLY` for a literal list and
    /// breaking nothing. This refuses the write itself, so the gap closes at
    /// the point the row would be created rather than at the point someone
    /// reads the code.
    #[test]
    fn a_snapshot_for_an_undeclared_component_is_refused() {
        let store = mem_store();
        let now = now();

        // Positive control first: a declared component stores, so a refusal
        // below means the name was rejected and not that the call never works.
        for declared in health_components() {
            store
                .insert_health_snapshot(declared, now, Some("operational"))
                .unwrap_or_else(|e| panic!("{declared} is declared and must store: {e}"));
        }

        let err = store
            .insert_health_snapshot("a_new_component", now, Some("operational"))
            .expect_err("an undeclared component must be refused");
        assert!(
            matches!(err, DbError::UndeclaredComponent(ref c) if c == "a_new_component"),
            "got {err}"
        );

        let rows: i64 = store
            .conn
            .query_row(
                "SELECT COUNT(*) FROM health_snapshots WHERE component = 'a_new_component'",
                [],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(rows, 0, "and nothing is stored for it");
    }

    /// The gate `rebuild-db.sh` and the rekey both apply: every row arrived,
    /// and every row that arrived is internally consistent. Tested as a rule
    /// rather than through the migration, because the current copy statement
    /// cannot produce either failure. See `rekey_is_verified`.
    #[test]
    fn a_rebuilt_table_replaces_the_original_only_when_it_verifies() {
        assert!(rekey_is_verified(1336, 1336, 0), "a complete, consistent copy swaps");
        assert!(rekey_is_verified(0, 0, 0), "an empty table is a complete copy of nothing");
        assert!(!rekey_is_verified(1336, 1335, 0), "one row short does not swap");
        assert!(!rekey_is_verified(1336, 1337, 0), "one row extra does not swap either");
        assert!(!rekey_is_verified(1336, 1336, 1), "one inconsistent row does not swap");
    }

    /// Builds a database file carrying the pre-rekey `kp_forecast` and returns
    /// its path. `rows` are `(ts, kp_e2, fetched_at)` in the old shape.
    fn old_shape_forecast_db(name: &str, rows: &[(i64, i64, i64)]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("astraeus-{name}-{}.duckdb", now()));
        let _ = std::fs::remove_file(&path);
        let conn = duckdb::Connection::open(&path).expect("open");
        conn.execute_batch(
            "CREATE TABLE kp_forecast (
                 ts             BIGINT NOT NULL PRIMARY KEY,
                 kp_e2          BIGINT NOT NULL,
                 ci_lower_e2    BIGINT,
                 ci_upper_e2    BIGINT,
                 uncertainty_e4 BIGINT,
                 fetched_at     BIGINT NOT NULL
             );",
        )
        .expect("old schema");
        // A database that has already been through the earlier migrations,
        // which is what production is. Without this the 2026-08 purge runs and
        // empties the table before the rekey ever sees it, and the test would
        // be describing a fresh file rather than the one being migrated.
        conn.execute_batch(
            "CREATE TABLE schema_migrations (
                 id         TEXT   NOT NULL PRIMARY KEY,
                 applied_at BIGINT NOT NULL
             );",
        )
        .expect("migrations table");
        conn.execute(
            "INSERT INTO schema_migrations (id, applied_at) VALUES (?, ?)",
            params![PURGE_FORECASTS_MIGRATION, now()],
        )
        .expect("record the purge");
        for (ts, kp, fetched) in rows {
            conn.execute(
                "INSERT INTO kp_forecast (ts, kp_e2, ci_lower_e2, ci_upper_e2, uncertainty_e4, fetched_at)
                 VALUES (?, ?, 100, 400, 1000, ?)",
                params![ts, kp, fetched],
            )
            .expect("seed row");
        }
        drop(conn);
        path
    }

    /// The 1336 rows this migration moves are the only forecast history that
    /// exists. Each is a 6 h prediction filed as 3 h: before 001cda9 the
    /// training loop paired a window with the target one period beyond the lead
    /// its head was published as. The value is real and the label was wrong, so
    /// the label moves, `ts` moves with it, and `model_sha` stays NULL so no
    /// figure describing the current model can pick them up.
    #[test]
    fn the_rekey_relabels_the_old_rows_as_six_hour_forecasts() {
        let issued = 1_700_000_000;
        let rows: Vec<(i64, i64, i64)> = (0..5)
            .map(|i| {
                let f = issued + i * 1800;
                (f + 3 * 3600, 300 + i, f)
            })
            .collect();
        let path = old_shape_forecast_db("rekey-ok", &rows);

        let store = Store::open(path.to_str().unwrap()).expect("migrated open");

        let (n, sixes, with_sha): (i64, i64, i64) = store
            .conn
            .query_row(
                "SELECT COUNT(*), \
                        COUNT(*) FILTER (WHERE horizon_hours = 6), \
                        COUNT(*) FILTER (WHERE model_sha IS NOT NULL) \
                 FROM kp_forecast",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .expect("counts");
        assert_eq!(n, rows.len() as i64, "every row survived");
        assert_eq!(sixes, n, "every migrated row is labelled 6h");
        assert_eq!(with_sha, 0, "and none of them claims a model");

        // Issue time is the old write time, and the target moved with the label.
        let inconsistent: i64 = store
            .conn
            .query_row(
                "SELECT COUNT(*) FROM kp_forecast WHERE ts != issued_at + horizon_hours * 3600",
                [],
                |r| r.get(0),
            )
            .expect("consistency");
        assert_eq!(inconsistent, 0, "ts is issue plus lead for every row");

        let (first_issue, first_ts): (i64, i64) = store
            .conn
            .query_row(
                "SELECT issued_at, ts FROM kp_forecast ORDER BY issued_at LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("first row");
        assert_eq!(first_issue, issued, "issue time is the old fetched_at");
        assert_eq!(first_ts, issued + 6 * 3600, "target is six hours after it");

        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    /// A rewrite that cannot complete leaves the original alone and refuses to
    /// start, rather than serving queries against a table that lost half its
    /// rows. The old key was the target time, so two rows could share a write
    /// second and collide on the new one: production has no such pair, and this
    /// is what happens to a database that does.
    #[test]
    fn a_rekey_that_cannot_complete_changes_nothing() {
        let issued = 1_700_000_000;
        // Two different targets written in the same second. Legal under the old
        // key, a collision under (issued_at, horizon_hours).
        let rows = [
            (issued + 3 * 3600, 300, issued),
            (issued + 3 * 3600 + 1, 310, issued),
            (issued + 3 * 3600 + 2, 320, issued + 1800),
        ];
        let path = old_shape_forecast_db("rekey-collide", &rows);

        let Err(err) = Store::open(path.to_str().unwrap()) else {
            panic!("a rekey that cannot complete must refuse to start");
        };
        assert!(
            matches!(err, DbError::Migration(_)),
            "a failed rekey is a migration failure, got {err}"
        );

        // The original table is still there, still in its old shape, still whole.
        let conn = duckdb::Connection::open(&path).expect("reopen");
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM kp_forecast", [], |r| r.get(0))
            .expect("count");
        assert_eq!(n, rows.len() as i64, "nothing was lost");
        let has_horizon: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM duckdb_columns() \
                 WHERE table_name = 'kp_forecast' AND column_name = 'horizon_hours'",
                [],
                |r| r.get(0),
            )
            .expect("columns");
        assert_eq!(has_horizon, 0, "the original shape is untouched");
        let leftover: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM duckdb_tables() WHERE table_name = 'kp_forecast_new'",
                [],
                |r| r.get(0),
            )
            .expect("tables");
        assert_eq!(leftover, 0, "the half built copy is not left behind to block a retry");

        drop(conn);
        let _ = std::fs::remove_file(&path);
    }

    /// Every horizon of one issue, or none. The old table held a single row per
    /// forecast keyed on the target time, so three of the four horizons the ml
    /// service returns were discarded on every call.
    #[test]
    fn kp_forecast_stores_every_horizon_of_an_issue() {
        let store = mem_store();
        let issued = 1_700_000_000;

        let points: Vec<ForecastPoint> = FORECAST_HORIZONS
            .iter()
            .map(|h| ForecastPoint {
                horizon_hours: *h,
                kp_e2: 200 + h * 10,
                ci_lower_e2: Some(180),
                ci_upper_e2: Some(320),
                uncertainty_e4: Some(1057),
            })
            .collect();
        store
            .insert_kp_forecast(issued, Some("abc123"), &points)
            .unwrap();

        for h in FORECAST_HORIZONS {
            let (ts, kp) = store.get_kp_forecast_latest(h).unwrap().expect("a row");
            assert_eq!(ts, issued + h * 3600, "{h}h target time is issue plus lead");
            assert_eq!(kp, 200 + h * 10, "{h}h keeps its own value");
        }

        // Re-issuing at the same second updates in place rather than colliding.
        let revised: Vec<ForecastPoint> = points
            .iter()
            .map(|p| ForecastPoint { kp_e2: 999, ..*p })
            .collect();
        store
            .insert_kp_forecast(issued, Some("abc123"), &revised)
            .unwrap();
        assert_eq!(store.get_kp_forecast_latest(3).unwrap(), Some((issued + 10800, 999)));
    }

    /// Two predictions can name the same instant. The 24 h head issued at 01:00
    /// and the 12 h head issued at 13:00 both target 01:00 the next day, and
    /// under a key of target time alone the second silently replaced the first.
    #[test]
    fn two_horizons_naming_the_same_instant_both_survive() {
        let store = mem_store();
        let early = 1_700_000_000;
        let late = early + 12 * 3600;

        let at = |kp: i64| -> Vec<ForecastPoint> {
            FORECAST_HORIZONS
                .iter()
                .map(|h| ForecastPoint {
                    horizon_hours: *h,
                    kp_e2: kp,
                    ci_lower_e2: None,
                    ci_upper_e2: None,
                    uncertainty_e4: None,
                })
                .collect()
        };
        store.insert_kp_forecast(early, Some("m"), &at(111)).unwrap();
        store.insert_kp_forecast(late, Some("m"), &at(222)).unwrap();

        let target = early + 24 * 3600;
        let rows: i64 = store
            .conn
            .query_row(
                "SELECT COUNT(*) FROM kp_forecast WHERE ts = ?",
                params![target],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(rows, 2, "the 24h call and the 12h call are different predictions");
    }

    /// A set that is not the published one is refused before anything is
    /// written. Four writes per cycle where there was one is four chances for a
    /// partial issue, and a history missing its 12 h rows on some cycles and not
    /// others hides in every aggregate that reads it.
    #[test]
    fn an_issue_missing_a_horizon_stores_nothing() {
        let store = mem_store();
        let issued = 1_700_000_000;

        let short: Vec<ForecastPoint> = [3, 6, 24]
            .iter()
            .map(|h| ForecastPoint {
                horizon_hours: *h,
                kp_e2: 300,
                ci_lower_e2: None,
                ci_upper_e2: None,
                uncertainty_e4: None,
            })
            .collect();
        let err = store.insert_kp_forecast(issued, None, &short).unwrap_err();
        assert!(
            matches!(err, DbError::PartialForecast { .. }),
            "a missing horizon must be refused, got {err}"
        );

        let rows: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM kp_forecast", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 0, "nothing at all is written, not the three that arrived");
    }

    /// The parser is the other half of that guarantee, and it is shared by both
    /// write sites. A payload missing a horizon yields no value the caller could
    /// store, rather than a shorter list it might not check.
    #[test]
    fn a_predict_payload_missing_a_horizon_yields_nothing() {
        let full = serde_json::json!({
            "model_sha256": "deadbeef",
            "forecast": FORECAST_HORIZONS.iter().map(|h| serde_json::json!({
                "horizon_hours": h,
                "predicted_kp": 3.25,
                "ci_lower": 2.0,
                "ci_upper": 4.5,
                "uncertainty": 0.5,
            })).collect::<Vec<_>>(),
        });
        let (points, sha) = ForecastPoint::from_predict_payload(&full).unwrap();
        assert_eq!(points.len(), FORECAST_HORIZONS.len());
        assert_eq!(sha.as_deref(), Some("deadbeef"));
        assert_eq!(points[0].kp_e2, 325);
        assert_eq!(points[0].uncertainty_e4, Some(5000));

        for drop in FORECAST_HORIZONS {
            let partial = serde_json::json!({
                "forecast": FORECAST_HORIZONS.iter().filter(|h| **h != drop)
                    .map(|h| serde_json::json!({
                        "horizon_hours": h, "predicted_kp": 3.0,
                    })).collect::<Vec<_>>(),
            });
            assert!(
                ForecastPoint::from_predict_payload(&partial).is_err(),
                "a payload without the {drop}h horizon must not parse"
            );
        }

        // The old shape, flat 3h fields and no forecast array, is not enough.
        let flat = serde_json::json!({ "predicted_kp": 3.0, "ci_lower": 2.0 });
        assert!(ForecastPoint::from_predict_payload(&flat).is_err());
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

        // Repeated to clear MIN_PAIRS_FOR_METRICS. The pattern is what the
        // assertions describe, so the averages are the same at 32 pairs as at 4.
        let repeats = 8;
        for i in 0..(cases.len() * repeats) {
            let (pred_e2, actual_kp) = cases[i % cases.len()];
            let issued = t + step * i as i64;
            let points: Vec<ForecastPoint> = FORECAST_HORIZONS
                .iter()
                .map(|h| ForecastPoint {
                    horizon_hours: *h,
                    kp_e2: pred_e2,
                    ci_lower_e2: None,
                    ci_upper_e2: None,
                    uncertainty_e4: Some(1000),
                })
                .collect();
            store.insert_kp_forecast(issued, Some("model-a"), &points).unwrap();
            store
                .insert_kp_3h_batch(&[Kp3hRecord {
                    time_tag: iso(issued + 3 * 3600),
                    kp: actual_kp,
                }])
                .unwrap();
        }

        let m = store.get_forecast_metrics(t - 1).unwrap();
        let three = m["horizons"]
            .as_array()
            .unwrap()
            .iter()
            .find(|h| h["horizon_hours"] == 3)
            .expect("the 3h horizon is always reported");

        assert_eq!(three["sufficient"], true);
        assert_eq!(three["n_samples"].as_i64().unwrap(), (cases.len() * repeats) as i64);
        assert_eq!(three["n_storms"].as_i64().unwrap(), (2 * repeats) as i64);
        assert_eq!(three["n_caught"].as_i64().unwrap(), repeats as i64);
        assert_eq!(three["n_false_pos"].as_i64().unwrap(), repeats as i64);
        assert!((three["hit_rate"].as_f64().unwrap() - 0.5).abs() < 1e-9);
        // MAE = mean(|pred-actual|) in Kp: (0.5+1.0+3.2+3.0)/4 = 1.925
        assert!((three["mae"].as_f64().unwrap() - 1.925).abs() < 1e-6);
        // RMSE = sqrt(mean(diff²)) in Kp ≈ 2.2633
        assert!((three["rmse"].as_f64().unwrap() - 2.263293).abs() < 1e-4);
        // mean σ = 1000/1e4 = 0.1
        assert!((three["mean_unc"].as_f64().unwrap() - 0.1).abs() < 1e-9);
    }

    /// Rows with no model_sha are the pre-001cda9 history: real 6 h forecasts,
    /// honestly relabelled, produced by a checkpoint that is not the one
    /// serving. They stay in the table and stay out of the accuracy figures,
    /// because a number averaging two models answers a question nobody asked.
    #[test]
    fn metrics_ignore_forecasts_from_an_unidentified_model() {
        let store = mem_store();
        let t = 1_700_000_000;

        for i in 0..40i64 {
            let issued = t + i * 3600;
            let points: Vec<ForecastPoint> = FORECAST_HORIZONS
                .iter()
                .map(|h| ForecastPoint {
                    horizon_hours: *h,
                    kp_e2: 900,
                    ci_lower_e2: None,
                    ci_upper_e2: None,
                    uncertainty_e4: None,
                })
                .collect();
            // No model_sha, exactly as the migration leaves the old rows.
            store.insert_kp_forecast(issued, None, &points).unwrap();
            store
                .insert_kp_3h_batch(&[Kp3hRecord {
                    time_tag: iso(issued + 3 * 3600),
                    kp: 1.0,
                }])
                .unwrap();
        }

        let m = store.get_forecast_metrics(t - 1).unwrap();
        for h in m["horizons"].as_array().unwrap() {
            assert_eq!(
                h["n_samples"].as_i64().unwrap(),
                0,
                "an unidentified model contributes no pairs at {}h",
                h["horizon_hours"]
            );
            assert_eq!(h["sufficient"], false);
        }
    }

    /// Below the floor a horizon reports its count and nothing else. The figure
    /// moves more under its own sampling noise than under the model at that
    /// size, and an empty cell invites no conclusion where a number would.
    #[test]
    fn a_horizon_under_the_floor_publishes_no_figure() {
        let store = mem_store();
        let t = 1_700_000_000;
        let short = MIN_PAIRS_FOR_METRICS - 1;

        for i in 0..short {
            let issued = t + i * 3600;
            let points: Vec<ForecastPoint> = FORECAST_HORIZONS
                .iter()
                .map(|h| ForecastPoint {
                    horizon_hours: *h,
                    kp_e2: 300,
                    ci_lower_e2: None,
                    ci_upper_e2: None,
                    uncertainty_e4: None,
                })
                .collect();
            store.insert_kp_forecast(issued, Some("model-a"), &points).unwrap();
            store
                .insert_kp_3h_batch(&[Kp3hRecord {
                    time_tag: iso(issued + 3 * 3600),
                    kp: 3.0,
                }])
                .unwrap();
        }

        let m = store.get_forecast_metrics(t - 1).unwrap();
        let three = m["horizons"]
            .as_array()
            .unwrap()
            .iter()
            .find(|h| h["horizon_hours"] == 3)
            .unwrap();
        assert_eq!(three["n_samples"].as_i64().unwrap(), short);
        assert_eq!(three["sufficient"], false);
        assert!(three["mae"].is_null(), "no figure below the floor");
        assert_eq!(three["min_samples"].as_i64().unwrap(), MIN_PAIRS_FOR_METRICS);
    }

    #[test]
    fn forecast_metrics_empty_window() {
        let store = mem_store();
        let m = store.get_forecast_metrics(0).unwrap();
        let horizons = m["horizons"].as_array().unwrap();
        // Every published horizon appears even with nothing stored, so a head
        // that stopped being written shows an empty row instead of vanishing.
        assert_eq!(horizons.len(), FORECAST_HORIZONS.len());
        for h in horizons {
            assert_eq!(h["n_samples"].as_i64().unwrap(), 0);
            assert_eq!(h["sufficient"], false);
        }
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
