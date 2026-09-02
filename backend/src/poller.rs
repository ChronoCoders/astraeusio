use std::sync::Arc;
use std::time::Duration;

use chrono::{Duration as ChronoDuration, NaiveDateTime, Utc};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use crate::{
    anomaly,
    db::Store,
    db_writer::{DbWriterHandle, WriteCmd},
    fetch::PollOutcome,
    iss, mailer, nasa, noaa, retry, starlink,
};

// ── Poller configuration ──────────────────────────────────────────────────────

pub struct PollerConfig {
    pub iss_interval: u64,
    pub kp_interval: u64,
    pub kp_3h_interval: u64,
    pub solar_wind_interval: u64,
    pub xray_interval: u64,
    pub alerts_interval: u64,
    pub neo_interval: u64,
    pub epic_interval: u64,
    pub apod_interval: u64,
    pub exoplanet_interval: u64,
    pub imf_interval: u64,
    pub dst_interval: u64,
    pub starlink_interval: u64,
    pub anomaly_interval: u64,
    pub forecast_interval: u64,
    /// The health snapshot loop. It writes no upstream data, which is why it
    /// was hardcoded at its call site and absent from this struct, and
    /// therefore absent from the boot line that every external check
    /// enumerates pollers from. `intervals` is now the one list and this is in
    /// it.
    pub health_interval: u64,
    /// The retention purge. Daily, because bounding growth is not urgent work
    /// and it competes with the pollers for DuckDB's single writer.
    pub retention_interval: u64,
    /// Total attempts per poll, including the first. Clamped to 1..=5, where 1
    /// means no retry. Parsed and then ignored until 2026-08-11, while both
    /// CLAUDE.md and README.md promised three attempts with backoff.
    pub retry_count: u32,
    /// The client's own timeout, needed to bound a single attempt.
    pub http_timeout: u64,
}

/// Seconds between health samples.
///
/// Module level because two callers need the same number: this poller, which
/// writes one sample per interval, and the uptime handler, which turns "samples
/// present" into a percentage and cannot do that without knowing how many were
/// due. A second `std::env::var` call in `routes.rs` would be the same rule
/// written twice, and the two would drift the first time the default moved.
pub fn health_interval_secs() -> u64 {
    std::env::var("HEALTH_INTERVAL")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300)
}

impl PollerConfig {
    pub fn from_env() -> Self {
        fn secs(key: &str, default: u64) -> u64 {
            std::env::var(key)
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(default)
        }
        Self {
            iss_interval: secs("ISS_INTERVAL", 5),
            kp_interval: secs("KP_INTERVAL", 60),
            kp_3h_interval: secs("KP_3H_INTERVAL", 1800),
            solar_wind_interval: secs("SOLAR_WIND_INTERVAL", 60),
            xray_interval: secs("XRAY_INTERVAL", 120),
            alerts_interval: secs("ALERTS_INTERVAL", 300),
            neo_interval: secs("NEO_INTERVAL", 1800),
            epic_interval: secs("EPIC_INTERVAL", 1800),
            apod_interval: secs("APOD_INTERVAL", 3600),
            exoplanet_interval: secs("EXOPLANET_INTERVAL", 86400),
            imf_interval: secs("IMF_INTERVAL", 60),
            dst_interval: secs("DST_INTERVAL", 300),
            starlink_interval: secs("STARLINK_INTERVAL", 3600),
            anomaly_interval: secs("ANOMALY_INTERVAL", 60),
            forecast_interval: secs("FORECAST_INTERVAL", 1800),
            health_interval: health_interval_secs(),
            retention_interval: secs("RETENTION_INTERVAL", 86_400),
            // 0 means off, so it clamps to a single attempt rather than
            // silently falling back to three. An unparseable value keeps the
            // default, because that is a typo and not an intent.
            retry_count: std::env::var("RETRY_COUNT")
                .ok()
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(3)
                .clamp(1, 5),
            http_timeout: std::env::var("HTTP_TIMEOUT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(60),
        }
    }
}

impl PollerConfig {
    /// Every poller `spawn` starts, and the interval it runs on.
    ///
    /// One list, for two readers that used to disagree. The boot line below is
    /// generated from this, and `poller-check.sh` on the host parses that line
    /// to know what rate each source should be delivering at, because a second
    /// copy of the table in bash would drift the first time anything here
    /// changed.
    ///
    /// The line it replaced was fifteen fields written out by hand against
    /// sixteen `tokio::spawn` calls: `health` was missing, so the one external
    /// check that enumerates pollers from something other than what has already
    /// spoken could not see it. `every_spawned_poller_is_in_the_interval_table`
    /// is what makes that unrepeatable, by reading the spawns out of this file
    /// rather than out of a log.
    ///
    /// Each name is the suffix of its `poll_` function, which is the invariant
    /// that test checks, and it is also the name the source logs under, which
    /// is what lets the host script line the two up.
    fn intervals(&self) -> [(&'static str, u64); 17] {
        [
            ("iss", self.iss_interval),
            ("kp", self.kp_interval),
            ("kp_3h", self.kp_3h_interval),
            ("solar_wind", self.solar_wind_interval),
            ("xray", self.xray_interval),
            ("alerts", self.alerts_interval),
            ("neo", self.neo_interval),
            ("epic", self.epic_interval),
            ("apod", self.apod_interval),
            ("exoplanets", self.exoplanet_interval),
            ("imf", self.imf_interval),
            ("dst", self.dst_interval),
            ("starlink", self.starlink_interval),
            ("anomaly", self.anomaly_interval),
            ("forecast", self.forecast_interval),
            ("health", self.health_interval),
            ("retention", self.retention_interval),
        ]
    }

    /// The interval table as the boot line prints it: `name=seconds`, space
    /// separated.
    ///
    /// A rendered interface, not a debug aid. `poller-check.sh` greps this line
    /// for `[a-z0-9_]+=[0-9]+` and treats every token but `retry_count` as a
    /// poller and its rate, so a change to this format is a change to something
    /// off this machine.
    fn intervals_line(&self) -> String {
        self.intervals()
            .iter()
            .map(|(name, secs)| format!("{name}={secs}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// One source's retry policy. The budget is that source's own interval, so
    /// a retry can never outlast the poll it belongs to.
    fn policy(&self, source: &'static str, interval: u64) -> retry::Policy {
        retry::Policy::new(source, self.retry_count, interval, self.http_timeout)
    }
}

// ── Spawn ─────────────────────────────────────────────────────────────────────

// Stagger initial poller startup to prevent DB mutex contention on first run.
// Each poller's first insert can be large (thousands of rows); if all fire at
// once the HTTP server is starved for 60+ seconds before any request lands.
pub fn spawn(
    client: reqwest::Client,
    db: Arc<Mutex<Store>>,
    writer: DbWriterHandle,
    smtp: Option<Arc<dyn mailer::Sender>>,
    ml_url: String,
) {
    let cfg = PollerConfig::from_env();
    // Emitted as `name=seconds` pairs in the message rather than as static
    // fields, because a static field list is exactly what went stale. The
    // shape is unchanged for anything parsing it: `retry_count` stays a field
    // and every other token is still `[a-z0-9_]+=[0-9]+`.
    info!(
        retry_count = cfg.retry_count,
        "poller: intervals loaded {}",
        cfg.intervals_line()
    );


    // Each policy is constructed once here and then moved into its poller, so
    // the line below reports what the running process actually computed rather
    // than a second derivation that could drift from it. The per-attempt
    // ceiling was previously only inferable from the inputs.
    let p_iss = cfg.policy("poller/iss", cfg.iss_interval);
    let p_kp = cfg.policy("poller/kp", cfg.kp_interval);
    let p_kp3h = cfg.policy("poller/kp-3h", cfg.kp_3h_interval);
    let p_solar_wind = cfg.policy("poller/solar-wind", cfg.solar_wind_interval);
    let p_xray = cfg.policy("poller/xray", cfg.xray_interval);
    let p_alerts = cfg.policy("poller/alerts", cfg.alerts_interval);
    let p_neo = cfg.policy("poller/neo", cfg.neo_interval);
    let p_epic = cfg.policy("poller/epic", cfg.epic_interval);
    let p_apod = cfg.policy("poller/apod", cfg.apod_interval);
    let p_exoplanets = cfg.policy("poller/exoplanets", cfg.exoplanet_interval);
    let p_imf = cfg.policy("poller/imf", cfg.imf_interval);
    let p_dst = cfg.policy("poller/dst", cfg.dst_interval);
    let p_starlink = cfg.policy("poller/starlink", cfg.starlink_interval);
    let p_forecast = cfg.policy("poller/forecast", cfg.forecast_interval);
    info!(
        retry_count = cfg.retry_count,
        http_timeout = cfg.http_timeout,
        "poller: attempt timeouts {}",
        [&p_iss, &p_kp, &p_kp3h, &p_solar_wind, &p_xray, &p_alerts, &p_neo, &p_epic, &p_apod, &p_exoplanets, &p_imf, &p_dst, &p_starlink, &p_forecast]
            .iter()
            .map(|p| format!(
                "{}={}s",
                p.source.trim_start_matches("poller/"),
                p.attempt_timeout.as_secs()
            ))
            .collect::<Vec<_>>()
            .join(" ")
    );

    // Tier 0 - tiny/read-only, start immediately
    tokio::spawn(poll_iss(
        client.clone(),
        writer.clone(),
        0,
        p_iss,
    ));
    tokio::spawn(poll_anomaly(
        db.clone(),
        writer.clone(),
        smtp,
        2,
        cfg.anomaly_interval,
    ));
    // Tier 1 - small inserts, 5-second spacing
    tokio::spawn(poll_kp(
        client.clone(),
        writer.clone(),
        5,
        p_kp,
    ));
    tokio::spawn(poll_alerts(
        client.clone(),
        writer.clone(),
        10,
        p_alerts,
    ));
    tokio::spawn(poll_neo(
        client.clone(),
        writer.clone(),
        15,
        p_neo,
    ));
    tokio::spawn(poll_epic(
        client.clone(),
        writer.clone(),
        20,
        p_epic,
    ));
    tokio::spawn(poll_apod(
        client.clone(),
        writer.clone(),
        25,
        p_apod,
    ));
    // Tier 2 - large initial inserts (hundreds to thousands of rows), 8-second spacing
    tokio::spawn(poll_kp_3h(
        client.clone(),
        writer.clone(),
        30,
        p_kp3h,
    ));
    tokio::spawn(poll_dst(
        client.clone(),
        writer.clone(),
        38,
        p_dst,
    ));
    tokio::spawn(poll_exoplanets(
        client.clone(),
        writer.clone(),
        46,
        p_exoplanets,
    ));
    tokio::spawn(poll_imf(
        client.clone(),
        writer.clone(),
        54,
        p_imf,
    ));
    tokio::spawn(poll_solar_wind(
        client.clone(),
        writer.clone(),
        62,
        p_solar_wind,
    ));
    tokio::spawn(poll_xray(
        client.clone(),
        writer.clone(),
        70,
        p_xray,
    ));
    // Tier 3 - Starlink: DELETE + 7000+ inserts in one transaction, start last
    tokio::spawn(poll_starlink(
        client.clone(),
        writer.clone(),
        90,
        p_starlink,
    ));
    // Forecast - calls the ML sidecar on a fixed cadence so kp_forecast builds
    // a continuous time series (the Forecast page chart + metrics depend on it).
    // Delayed past first Kp ingest so recent readings exist for the request.
    tokio::spawn(poll_forecast(
        client.clone(),
        db.clone(),
        writer.clone(),
        ml_url.clone(),
        45,
        p_forecast,
    ));
    // Health snapshots - record per-component status every 5 minutes for the
    // status page's 90-day uptime strip.
    // Runs five minutes in, so a restart loop cannot spend its time purging.
    tokio::spawn(poll_retention(
        db.clone(),
        300,
        cfg.retention_interval,
    ));
    tokio::spawn(poll_health(
        client.clone(),
        db.clone(),
        writer.clone(),
        ml_url,
        60,
        cfg.health_interval,
    ));
}

// ── Poll functions ────────────────────────────────────────────────────────────

async fn poll_iss(
    client: reqwest::Client,
    writer: DbWriterHandle,
    init_delay_secs: u64,
    policy: retry::Policy,
) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        if let Some(pos) = retry::run(&policy, || iss::fetch_iss_position(&client)).await {
            info!(
                "poller/iss: lat={:.4} lon={:.4}",
                pos.latitude, pos.longitude
            );
            writer.fire(WriteCmd::Iss(pos));
        }
        tokio::time::sleep(policy.budget).await;
    }
}

/// Logs how a poll ended, at a level that matches what it means.
///
/// The point is that the three ways a poll can come back with nothing no longer
/// share a line. A no-change response is routine, an empty payload is odd but
/// possible, and a payload whose every row failed to parse is a broken feed
/// contract. That last one logs at ERROR deliberately: the hourly poller check
/// on the host greps for ERROR, so the case that silently froze the IMF table
/// for forty days now reaches the alert on its own.
fn log_poll(source: &str, unit: &str, outcome: PollOutcome) {
    match outcome {
        PollOutcome::NoChange => {
            info!(source, "upstream reports no change, existing rows kept")
        }
        PollOutcome::EmptyPayload => {
            warn!(source, "upstream returned an empty payload, nothing written")
        }
        PollOutcome::Parsed { received, kept: 0 } => error!(
            source,
            received, "every row failed to parse, the feed shape has probably changed"
        ),
        PollOutcome::Parsed { received, kept } if kept < received => warn!(
            source,
            received,
            kept,
            dropped = received - kept,
            "some rows failed to parse"
        ),
        PollOutcome::Parsed { kept, .. } => info!("{source}: {kept} {unit}"),
    }
}

async fn poll_kp(
    client: reqwest::Client,
    writer: DbWriterHandle,
    init_delay_secs: u64,
    policy: retry::Policy,
) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        if let Some(fetched) = retry::run(&policy, || noaa::fetch_kp(&client)).await {
            log_poll("poller/kp", "records", fetched.outcome);
            writer.fire(WriteCmd::Kp(fetched.items));
        }
        tokio::time::sleep(policy.budget).await;
    }
}

async fn poll_kp_3h(
    client: reqwest::Client,
    writer: DbWriterHandle,
    init_delay_secs: u64,
    policy: retry::Policy,
) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        if let Some(fetched) = retry::run(&policy, || noaa::fetch_kp_3h(&client)).await {
            log_poll("poller/kp-3h", "records", fetched.outcome);
            writer.fire(WriteCmd::Kp3h(fetched.items));
        }
        tokio::time::sleep(policy.budget).await;
    }
}

async fn poll_solar_wind(
    client: reqwest::Client,
    writer: DbWriterHandle,
    init_delay_secs: u64,
    policy: retry::Policy,
) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        if let Some(fetched) = retry::run(&policy, || noaa::fetch_solar_wind(&client)).await {
            log_poll("poller/solar-wind", "records", fetched.outcome);
            writer.fire(WriteCmd::SolarWind(fetched.items));
        }
        tokio::time::sleep(policy.budget).await;
    }
}

async fn poll_xray(
    client: reqwest::Client,
    writer: DbWriterHandle,
    init_delay_secs: u64,
    policy: retry::Policy,
) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        if let Some(fetched) = retry::run(&policy, || noaa::fetch_xray(&client)).await {
            log_poll("poller/xray", "records", fetched.outcome);
            writer.fire(WriteCmd::Xray(fetched.items));
        }
        tokio::time::sleep(policy.budget).await;
    }
}

/// How long the alerts feed may be quiet before the newest product it is
/// serving is evidence of a stall rather than of calm space weather.
///
/// Measured rather than chosen, because the last threshold picked by intuition
/// was the six hour one on `kp_3h`, which flapped for half an hour in every
/// three and took the forecast down with it.
///
/// Over the stored history to 2026-08-31, 2026-04-10 to 2026-08-30, 142 days
/// and 491 gaps between consecutive products: median gap 1.68 h, p95 26.4 h,
/// p99 62.6 h, longest 97.8 h. Thirty two gaps ran over a day, eleven over two
/// days, four over three.
///
/// Seven days is 1.7 times the longest quiet stretch ever observed here. It is
/// a margin over a thin tail rather than a bound derived from a distribution:
/// the four samples past 72 h all come from one stretch of one solar cycle, and
/// quiet periods lengthen towards solar minimum. Re-derive it from a year, and
/// treat a single false degraded during a genuinely calm week as the expected
/// cost of the current sample rather than as a bug.
const ALERT_QUIET_HORIZON_SECS: i64 = 7 * 86_400;

/// NOAA issues an alert when something happens, so the age of the newest row
/// cannot say whether the feed is alive. What can: whether our poll returned
/// anything at all, and whether the rolling window it returned is still being
/// added to.
///
/// An empty payload is degraded. The feed carries a rolling window of recent
/// products rather than only new ones, so a successful fetch returning nothing
/// is a fault, not a quiet sun.
///
/// A payload whose timestamps this cannot read counts as operational. The fetch
/// worked and the rows are stored either way, and reporting the feed dead
/// because NOAA changed a date format would be a worse answer than reporting it
/// alive.
fn alerts_liveness(items: &[noaa::SpaceWeatherAlert], now: i64) -> &'static str {
    if items.is_empty() {
        return "degraded";
    }
    match items
        .iter()
        .filter_map(|a| parse_issue_datetime(&a.issue_datetime))
        .max()
    {
        None => "operational",
        Some(newest) if now - newest > ALERT_QUIET_HORIZON_SECS => "degraded",
        Some(_) => "operational",
    }
}

/// `2026-08-30 05:55:38.333` as stored, with the `T` form and a missing
/// fraction both accepted because the feed's format is NOAA's to change.
fn parse_issue_datetime(raw: &str) -> Option<i64> {
    ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S%.f"]
        .iter()
        .find_map(|f| NaiveDateTime::parse_from_str(raw.trim(), f).ok())
        .map(|dt| dt.and_utc().timestamp())
}

async fn poll_alerts(
    client: reqwest::Client,
    writer: DbWriterHandle,
    init_delay_secs: u64,
    policy: retry::Policy,
) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        let now = Utc::now().timestamp();
        // Recorded every cycle, not only on failure, so the status page can
        // tell a feed that is quiet from one that is gone. Nothing else can:
        // the table has no freshness entry, and a hard failure is visible only
        // as an ERROR line that no user surface reads.
        let verdict = match retry::run(&policy, || noaa::fetch_alerts(&client)).await {
            Some(fetched) => {
                log_poll("poller/alerts", "alerts", fetched.outcome);
                let verdict = alerts_liveness(&fetched.items, now);
                writer.fire(WriteCmd::Alerts(fetched.items));
                verdict
            }
            // retry::run has already logged why it failed.
            None => "degraded",
        };
        writer.fire(WriteCmd::HealthSnapshot {
            component: "noaa_alerts".to_string(),
            ts: now,
            status: Some(verdict.to_string()),
        });
        tokio::time::sleep(policy.budget).await;
    }
}

async fn poll_neo(
    client: reqwest::Client,
    writer: DbWriterHandle,
    init_delay_secs: u64,
    policy: retry::Policy,
) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        let today = Utc::now().date_naive();
        let start = today.format("%Y-%m-%d").to_string();
        let end = (today + ChronoDuration::days(7))
            .format("%Y-%m-%d")
            .to_string();
        if let Some(feed) = retry::run(&policy, || nasa::fetch_neo_feed(&client, &start, &end)).await {
            log_poll(
                "poller/neo",
                "objects",
                PollOutcome::strict(feed.element_count as usize),
            );
            let fetched_at = Utc::now().timestamp();
            writer.fire(WriteCmd::Neo(Box::new(feed), fetched_at));
        }
        tokio::time::sleep(policy.budget).await;
    }
}

async fn poll_epic(
    client: reqwest::Client,
    writer: DbWriterHandle,
    init_delay_secs: u64,
    policy: retry::Policy,
) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        if let Some(images) = retry::run(&policy, || nasa::fetch_epic(&client)).await {
            log_poll("poller/epic", "images", PollOutcome::strict(images.len()));
            writer.fire(WriteCmd::Epic(images));
        }
        tokio::time::sleep(policy.budget).await;
    }
}

async fn poll_apod(
    client: reqwest::Client,
    writer: DbWriterHandle,
    init_delay_secs: u64,
    policy: retry::Policy,
) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        if let Some(apod) = retry::run(&policy, || nasa::fetch_apod(&client)).await {
            info!("poller/apod: {}", apod.date);
            writer.fire(WriteCmd::Apod(apod));
        }
        tokio::time::sleep(policy.budget).await;
    }
}

async fn poll_exoplanets(
    client: reqwest::Client,
    writer: DbWriterHandle,
    init_delay_secs: u64,
    policy: retry::Policy,
) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        if let Some(planets) = retry::run(&policy, || nasa::fetch_exoplanets(&client)).await {
            log_poll("poller/exoplanets", "planets", PollOutcome::strict(planets.len()));
            writer.fire(WriteCmd::Exoplanets(planets));
        }
        tokio::time::sleep(policy.budget).await;
    }
}

async fn poll_imf(
    client: reqwest::Client,
    writer: DbWriterHandle,
    init_delay_secs: u64,
    policy: retry::Policy,
) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        if let Some(fetched) = retry::run(&policy, || noaa::fetch_imf(&client)).await {
            log_poll("poller/imf", "records", fetched.outcome);
            writer.fire(WriteCmd::Imf(fetched.items));
        }
        tokio::time::sleep(policy.budget).await;
    }
}

async fn poll_dst(
    client: reqwest::Client,
    writer: DbWriterHandle,
    init_delay_secs: u64,
    policy: retry::Policy,
) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        if let Some(fetched) = retry::run(&policy, || noaa::fetch_dst(&client)).await {
            log_poll("poller/dst", "records", fetched.outcome);
            writer.fire(WriteCmd::Dst(fetched.items));
        }
        tokio::time::sleep(policy.budget).await;
    }
}

async fn poll_starlink(
    client: reqwest::Client,
    writer: DbWriterHandle,
    init_delay_secs: u64,
    policy: retry::Policy,
) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        if let Some(fetched) = retry::run(&policy, || starlink::fetch_starlink(&client)).await {
            log_poll("poller/starlink", "satellites", fetched.outcome);
            writer.fire(WriteCmd::Starlink(fetched.items));
        }
        tokio::time::sleep(policy.budget).await;
    }
}

async fn poll_forecast(
    client: reqwest::Client,
    db: Arc<Mutex<Store>>,
    writer: DbWriterHandle,
    ml_url: String,
    init_delay_secs: u64,
    policy: retry::Policy,
) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    let ml_timeout = std::env::var("ML_TIMEOUT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    loop {
        // seq_len comes from the ML checkpoint, so it is fetched each cycle
        // rather than duplicated as a constant here.
        let seq_len = match crate::routes::ml_seq_len(&client, &ml_url).await {
            Ok(n) => n,
            Err(e) => {
                error!(source = "poller/forecast", "ml seq_len: {}", crate::redact::secrets(&e.to_string()));
                tokio::time::sleep(policy.budget).await;
                continue;
            }
        };
        // Read recent Kp under the lock, then release before the HTTP call.
        let readings = {
            let guard = db.lock().await;
            guard.get_recent_kp_3h(seq_len)
        };
        match readings {
            Ok(r) if !r.is_empty() => {
                // error_for_status inside the closure so a 5xx from the sidecar
                // is a retryable failure rather than a success carrying a bad
                // status. The forecast went down once for a transient reason,
                // and it is the same loop and the same failure shape as the
                // feeds above.
                let resp = retry::run(&policy, || {
                    client
                        .post(format!("{ml_url}/predict"))
                        .timeout(Duration::from_secs(ml_timeout))
                        .json(&serde_json::json!({ "readings": &r }))
                        .send()
                })
                .await
                .map(|resp| resp.error_for_status());
                match resp {
                    Some(Ok(resp)) => match resp
                        .json::<serde_json::Value>()
                        .await
                    {
                        Ok(payload) => match crate::db::ForecastPoint::from_predict_payload(&payload) {
                            Ok((points, model_sha)) => {
                                let issued_at = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs() as i64;
                                let near = points
                                    .iter()
                                    .find(|p| p.horizon_hours == 3)
                                    .map(|p| p.kp_e2 as f64 / 100.0)
                                    .unwrap_or_default();
                                writer.fire(WriteCmd::KpForecast {
                                    issued_at,
                                    model_sha,
                                    points,
                                });
                                info!(
                                    horizons = ?crate::db::FORECAST_HORIZONS,
                                    "poller/forecast: predicted Kp {near:.2} @ +3h"
                                );
                            }
                            // Nothing stored. A cycle that answered with three
                            // of four horizons leaves a hole in one series and
                            // not the others, which no aggregate would show.
                            Err(e) => error!(source = "poller/forecast", "{e}"),
                        },
                        Err(e) => error!(source = "poller/forecast", "parse: {}", crate::redact::secrets(&e.to_string())),
                    },
                    // A status that survived every attempt, so it is not
                    // transient. retry::run already reported a failure to
                    // reach the sidecar at all.
                    Some(Err(e)) => error!(
                        source = "poller/forecast",
                        "ml status: {}",
                        crate::redact::secrets(&e.to_string())
                    ),
                    None => {}
                }
            }
            Ok(_) => info!("poller/forecast: no Kp data yet, skipping"),
            Err(e) => error!(source = "poller/forecast", "db: {}", crate::redact::secrets(&e.to_string())),
        }
        tokio::time::sleep(policy.budget).await;
    }
}

async fn poll_anomaly(
    db: Arc<Mutex<Store>>,
    writer: DbWriterHandle,
    smtp: Option<Arc<dyn mailer::Sender>>,
    init_delay_secs: u64,
    interval: u64,
) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        {
            let db_guard = db.lock().await;
            if let Err(e) = anomaly::detect_and_store(&db_guard, &writer) {
                error!(source = "poller/anomaly", "detect: {e}");
            }
        }
        if let Some(ref cfg) = smtp {
            let _ = dispatch_email_alerts(&db, &writer, cfg.as_ref()).await;
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
}

/// Returns how many cooldowns it recorded, which is how many alerts actually
/// went out.
///
/// Returned rather than discarded so the decision is observable without reading
/// it back out of the database. The write goes through `DbWriterHandle`, whose
/// store is a different connection, so a test that asserted on the database
/// would be asserting on a copy nothing wrote to.
async fn dispatch_email_alerts(
    db: &Arc<Mutex<Store>>,
    writer: &DbWriterHandle,
    sender: &dyn mailer::Sender,
) -> usize {
    // Gather data while holding lock, then release before any async work.
    let (kp_opt, wind_opt, subs) = {
        let guard = db.lock().await;
        let kp = guard.latest_kp_raw().unwrap_or(None);
        let wind = guard.latest_solar_wind_speed_raw().unwrap_or(None);
        let subs = guard.list_enabled_email_alerts().unwrap_or_default();
        (kp, wind, subs)
    };

    let now_ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    const COOLDOWN_SECS: i64 = 3600;

    let mut notified = 0usize;
    for sub in subs {
        if let Some(last) = sub.last_notified_at
            && now_ts - last < COOLDOWN_SECS
        {
            continue;
        }

        let lines = alert_lines(&kp_opt, &wind_opt, &sub, now_ts);
        if lines.is_empty() {
            continue;
        }

        let email = sub.user_email.clone();
        let writer = writer.clone();
        let body = format!(
            "Space Weather Alert\n\nThe following conditions have exceeded your thresholds:\n\n{}\n\nView your dashboard: https://astraeusio.com\n\nTo update alert settings, visit the API Keys page in your dashboard.",
            lines.join("\n")
        );
        // Awaited rather than spawned, so the cooldown is written before the
        // next subscription is considered and a test can observe the outcome.
        // The loop is over a handful of subscriptions on a 60 s cycle, so the
        // ordering costs nothing worth keeping the race for.
        //
        // The cooldown records that an alert was delivered, so it is marked on
        // the strength of the send rather than before it. Marking first meant a
        // failed send bought an hour of silence exactly as a successful one did,
        // and the user heard about neither.
        if sender
            .send_text(&email, "Astraeusio Space Weather Alert", &body)
            .await
        {
            writer.fire(WriteCmd::TouchEmailAlertNotified(email));
            notified += 1;
        }
    }
    notified
}

/// Which of a subscription's thresholds the current readings have crossed.
///
/// Pulled out of the dispatcher so the rule can be asserted. Left inline, the
/// freshness bound was testable as a function and unguarded as a decision:
/// mutation testing on 2026-09-01 removed its use from the dispatcher and no
/// test noticed, which is the same shape as the auxiliary filter in the health
/// handler earlier the same day.
///
/// A reading older than its own `SERIES_FRESHNESS` limit produces no line. The
/// email says conditions have exceeded your thresholds, present tense, and a
/// reading from eleven hours ago cannot support that sentence (AUD-028).
fn alert_lines(
    kp: &Option<(String, i64, i64)>,
    wind: &Option<(String, i64, i64)>,
    sub: &crate::db::EmailAlertRow,
    now_ts: i64,
) -> Vec<String> {
    let mut lines = Vec::new();

    if let Some((_, observed_at, kp_e2)) = kp
        && crate::db::reading_is_current("noaa_kp", *observed_at, now_ts)
        && *kp_e2 >= sub.kp_threshold_e2
    {
        let kp = *kp_e2 as f64 / 100.0;
        let thr = sub.kp_threshold_e2 as f64 / 100.0;
        lines.push(format!("• Kp index {kp:.1} (your threshold: {thr:.1})"));
    }

    if let Some((_, observed_at, speed_e1)) = wind
        && crate::db::reading_is_current("noaa_solar_wind", *observed_at, now_ts)
        && *speed_e1 >= sub.wind_threshold_e1
    {
        let speed = *speed_e1 as f64 / 10.0;
        let thr = sub.wind_threshold_e1 as f64 / 10.0;
        lines.push(format!(
            "• Solar wind {speed:.0} km/s (your threshold: {thr:.0} km/s)"
        ));
    }

    lines
}

// ── Retention ─────────────────────────────────────────────────────────────────

/// Deletes rows past their table's window, once a day.
///
/// Daily rather than hourly because nothing here is urgent: the point is to
/// bound growth, not to keep the file at a particular size, and a purge that
/// runs while the pollers are writing competes for the one writer DuckDB
/// allows. The first run after a long gap can delete a lot, which is why the
/// log line names the tables and the counts.
async fn poll_retention(db: Arc<Mutex<Store>>, init_delay_secs: u64, interval: u64) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        let purged = { db.lock().await.purge_expired() };
        match purged {
            Ok(rows) if rows.is_empty() => {
                info!("poller/retention: nothing past its window");
            }
            Ok(rows) => {
                let summary = rows
                    .iter()
                    .map(|(table, n)| format!("{table}={n}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                info!("poller/retention: removed {summary}");
            }
            Err(e) => error!(source = "poller/retention", "purge: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
}

// ── Health snapshot poller ────────────────────────────────────────────────────

async fn poll_health(
    client: reqwest::Client,
    db: Arc<Mutex<Store>>,
    writer: DbWriterHandle,
    ml_url: String,
    init_delay_secs: u64,
    interval: u64,
) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let ml_status = match client
            .get(format!("{ml_url}/health"))
            .timeout(Duration::from_secs(3))
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => "operational",
            _ => "degraded",
        };

        let (series, celestrak_ts) = {
            let guard = db.lock().await;
            let celestrak = guard.external_freshness();
            (guard.series_health(), celestrak)
        };

        fn component_status(last: Option<i64>, now: i64, stale_secs: i64) -> &'static str {
            match last {
                None => "unknown",
                Some(t) if now - t > stale_secs => "degraded",
                Some(_) => "operational",
            }
        }

        let celestrak_status = component_status(celestrak_ts, now, 14_400);

        // Each NOAA series records its own history, so a feed that stops shows
        // as its own gap on the status page instead of being averaged away.
        for (component, status, _) in &series {
            writer.fire(WriteCmd::HealthSnapshot {
                component: (*component).to_string(),
                ts: now,
                status: Some((*status).to_string()),
            });
        }

        for (component, status) in [("ml_forecast", ml_status), ("celestrak", celestrak_status)] {
            writer.fire(WriteCmd::HealthSnapshot {
                component: component.to_string(),
                ts: now,
                status: Some(status.to_string()),
            });
        }

        // No status. `backend_api` recorded the literal "operational" and
        // `database` recorded a value that was "operational" unless the whole
        // database was empty, both written by the process under observation.
        // Neither column could ever say anything, so the row alone is the
        // record now, and the missing rows are what an outage looks like.
        for component in crate::db::LIVENESS_ONLY {
            writer.fire(WriteCmd::HealthSnapshot {
                component: component.to_string(),
                ts: now,
                status: None,
            });
        }

        let degraded: Vec<&str> = series
            .iter()
            .filter(|(_, status, _)| *status != "operational")
            .map(|(component, _, _)| *component)
            .collect();
        info!(
            "poller/health: snapshot recorded (ml={ml_status} \
             celestrak={celestrak_status} series_not_operational={degraded:?})"
        );

        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// The cooldown is an hour of silence, so it may only be spent on an alert
    /// that actually went out.
    ///
    /// `a133524` fixed the ordering and left it untested, because the send was
    /// constructed inside the function that sends and no test could make it
    /// fail. That is what the `Sender` seam is for. Both directions are here:
    /// a successful send records the cooldown, a failed one does not, and a
    /// test asserting only the first would pass if the cooldown were written
    /// unconditionally, which is the defect being guarded.
    #[tokio::test]
    async fn the_cooldown_is_spent_only_on_an_alert_that_was_sent() {
        use crate::mailer::TestSender;

        async fn run(sender: std::sync::Arc<TestSender>) -> (usize, usize) {
            let store = Store::open(":memory:").expect("store");
            let email = "alerts@example.com";
            store.create_user(email, "hash").expect("user");
            // Kp far above the threshold, observed now so the freshness rule
            // does not suppress the line.
            let now = chrono::Utc::now();
            store
                .insert_kp_batch(&[crate::noaa::KpRecord {
                    time_tag: now.format("%Y-%m-%dT%H:%M:%S").to_string(),
                    kp_index: 9,
                    estimated_kp: 9.0,
                }])
                .expect("kp");
            store
                .upsert_email_alert("sub-1", email, true, 500, 100_000)
                .expect("subscription");

            let client = reqwest::Client::new();
            let db = Arc::new(Mutex::new(store));
            let writer = crate::db_writer::spawn(
                Store::open(":memory:").expect("writer store"),
                client,
            );
            let notified = dispatch_email_alerts(&db, &writer, sender.as_ref()).await;
            (sender.count(), notified)
        }

        let refusing = std::sync::Arc::new(TestSender::refusing());
        let (attempted, marked) = run(refusing).await;
        assert_eq!(attempted, 1, "the alert must be attempted");
        assert_eq!(marked, 0, "a failed send must not buy an hour of silence");

        // The other direction, and it is not decoration: the assertion above
        // also holds if the cooldown is never written at all, which would mean
        // an alert every cycle forever.
        let accepting = std::sync::Arc::new(TestSender::accepting());
        let (attempted, marked) = run(accepting).await;
        assert_eq!(attempted, 1, "the alert must be attempted");
        assert_eq!(marked, 1, "a delivered alert must record its cooldown");
    }

    /// The interval table must name every poller that exists, not every poller
    /// that has spoken.
    ///
    /// This is the same hole as `poller/anomaly`, one level down. That one sat
    /// unmapped because `poller-check.sh` built its list from the log and the
    /// anomaly detector writes to the log only when it finds something. The fix
    /// was to enumerate from the backend's interval line instead, which named
    /// fifteen pollers against sixteen `tokio::spawn` calls, so `health` was
    /// invisible to the check that exists to catch invisible pollers. A list
    /// written out by hand cannot be trusted to describe the code beside it.
    ///
    /// So this reads the spawns out of the source rather than out of a log or a
    /// running process, and fails when a poller is added without an entry, or
    /// an entry is left behind after a poller is removed.
    #[test]
    fn every_spawned_poller_is_in_the_interval_table() {
        // Assembled rather than written out, so this test's own source does not
        // match the pattern it scans for.
        let needle = format!("tokio::spawn(poll{}", '_');
        let src = include_str!("poller.rs");

        let spawned: BTreeSet<&str> = src
            .match_indices(needle.as_str())
            .filter_map(|(at, matched)| src[at + matched.len()..].split('(').next())
            .filter(|name| {
                !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            })
            .collect();

        assert!(
            spawned.len() > 10,
            "the scan found {} spawned pollers, so it has stopped matching the              source rather than found a real change: {spawned:?}",
            spawned.len()
        );

        let declared: BTreeSet<&str> = PollerConfig::from_env()
            .intervals()
            .iter()
            .map(|(name, _)| *name)
            .collect();

        let missing: Vec<&&str> = spawned.difference(&declared).collect();
        let extra: Vec<&&str> = declared.difference(&spawned).collect();
        assert!(
            missing.is_empty() && extra.is_empty(),
            "the interval table and the spawns disagree.              spawned with no entry: {missing:?}.              entry with no spawn: {extra:?}.              Every poller belongs in PollerConfig::intervals, including one that              fetches nothing, because that table is what every external check              enumerates pollers from."
        );
    }

    fn subscription() -> crate::db::EmailAlertRow {
        crate::db::EmailAlertRow {
            user_email: "watcher@example.com".to_owned(),
            enabled: true,
            kp_threshold_e2: 500,       // Kp 5.0
            wind_threshold_e1: 6000,    // 600 km/s
            last_notified_at: None,
        }
    }

    /// The email says conditions have exceeded your thresholds, present tense.
    /// A reading old enough to be wrong about now must not produce one, or a
    /// feed that goes quiet mid-storm re-sends its last value every hour for as
    /// long as it stays dead (AUD-028).
    #[test]
    fn a_stale_reading_produces_no_alert_however_high_it_is() {
        let now = 1_800_000_000;
        let sub = subscription();
        let storm = |age: i64| Some(("t".to_owned(), now - age, 900i64));   // Kp 9.0
        let gale = |age: i64| Some(("t".to_owned(), now - age, 9_000i64));  // 900 km/s

        // Fresh and over threshold: both lines.
        let lines = alert_lines(&storm(60), &gale(60), &sub, now);
        assert_eq!(lines.len(), 2, "fresh readings over threshold must alert: {lines:?}");

        // The same readings, eleven hours old, which is what a dead feed looks
        // like. Kp allows 1800s and so does solar wind.
        let lines = alert_lines(&storm(40_000), &gale(40_000), &sub, now);
        assert!(lines.is_empty(), "a stale reading must not alert: {lines:?}");

        // One fresh and one stale: only the fresh one speaks.
        let lines = alert_lines(&storm(60), &gale(40_000), &sub, now);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("Kp"), "{lines:?}");

        // Fresh but under threshold stays silent, which is the ordinary case.
        let quiet = Some(("t".to_owned(), now - 60, 100i64));
        assert!(alert_lines(&quiet, &None, &sub, now).is_empty());
    }

    fn alert(issue: &str) -> noaa::SpaceWeatherAlert {
        noaa::SpaceWeatherAlert {
            product_id: "K05".to_owned(),
            issue_datetime: issue.to_owned(),
            message: "Space Weather Message".to_owned(),
        }
    }

    /// The whole point of the liveness verdict: it has to stay operational
    /// through a quiet stretch longer than anything NOAA has actually left,
    /// because a threshold that fires on calm space weather is worse than no
    /// threshold at all.
    #[test]
    fn a_quiet_feed_is_not_a_dead_feed() {
        let now = 1_756_600_000;
        let hours = |h: i64| now - h * 3_600;

        // 97.8 h is the longest gap in the stored history. A verdict that
        // cannot survive it would have cried wolf in August 2026.
        for quiet_hours in [0, 2, 27, 63, 98, 120, 167] {
            assert_eq!(
                alerts_liveness(
                    &[alert(&fmt_issue(hours(quiet_hours)))],
                    now
                ),
                "operational",
                "{quiet_hours} h of quiet is normal for this feed"
            );
        }
    }

    /// Past the measured horizon the feed is serving a window it has stopped
    /// adding to, which no other signal in this codebase can see.
    #[test]
    fn a_feed_stuck_past_the_horizon_is_degraded() {
        let now = 1_756_600_000;
        for quiet_hours in [169, 240, 24 * 40] {
            assert_eq!(
                alerts_liveness(
                    &[alert(&fmt_issue(now - quiet_hours * 3_600))],
                    now
                ),
                "degraded",
                "{quiet_hours} h without a product is past the horizon"
            );
        }
    }

    /// A successful fetch returning nothing is a fault. The feed carries a
    /// rolling window of recent products, not only new ones, so empty is never
    /// what a healthy quiet period looks like.
    #[test]
    fn an_empty_payload_is_degraded() {
        assert_eq!(alerts_liveness(&[], 1_756_600_000), "degraded");
    }

    /// A date format we cannot read is not evidence the feed is dead. The rows
    /// still arrive and are still stored.
    #[test]
    fn an_unreadable_timestamp_does_not_condemn_the_feed() {
        let now = 1_756_600_000;
        assert_eq!(alerts_liveness(&[alert("30 August 2026, 05:55Z")], now), "operational");
    }

    /// Both spellings NOAA has used, with and without the fraction.
    #[test]
    fn the_issue_timestamp_parses_in_the_forms_the_feed_uses() {
        let expected = parse_issue_datetime("2026-08-30 05:55:38.333");
        assert!(expected.is_some());
        assert_eq!(parse_issue_datetime("2026-08-30T05:55:38.333"), expected);
        assert_eq!(parse_issue_datetime("2026-08-30 05:55:38"), expected);
        assert_eq!(parse_issue_datetime("not a date"), None);
    }

    fn fmt_issue(ts: i64) -> String {
        chrono::DateTime::from_timestamp(ts, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S%.3f").to_string())
            .unwrap_or_default()
    }

    /// The boot line is parsed off this machine, so both its content and its
    /// shape are part of the contract rather than a formatting choice.
    /// `poller-check.sh` takes every `[a-z0-9_]+=[0-9]+` token on the line,
    /// drops `retry_count`, and reads the rest as poller and rate.
    ///
    /// The content half was missing until 2026-08-31: this asserted sixteen
    /// tokens of the right shape and nothing about what was in them, and
    /// replacing `intervals_line` with a fixed string of sixteen invented
    /// tokens passed the whole suite. A line naming the wrong pollers, or the
    /// right ones at the wrong rates, would have been caught by nothing, and
    /// the host would have gone on computing expected poll counts from it.
    #[test]
    fn the_interval_line_stays_parseable_by_the_host_check() {
        let cfg = PollerConfig::from_env();
        let line = cfg.intervals_line();
        let tokens: Vec<&str> = line.split(' ').collect();

        // Content: every poller in the table, at its own interval, in order,
        // and nothing else on the line.
        let expected: Vec<String> = cfg
            .intervals()
            .iter()
            .map(|(name, secs)| format!("{name}={secs}"))
            .collect();
        let expected: Vec<&str> = expected.iter().map(String::as_str).collect();
        assert_eq!(
            tokens, expected,
            "the rendered line must be the interval table and nothing else"
        );

        // Shape: what the host script's regex can actually read.
        for token in tokens {
            let (name, secs) = token
                .split_once('=')
                .unwrap_or_else(|| panic!("token {token} is not name=value in: {line}"));
            assert!(
                !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "{name} does not match the [a-z0-9_]+ the host check greps for"
            );
            assert!(
                !secs.is_empty() && secs.chars().all(|c| c.is_ascii_digit()),
                "{secs} is not the bare integer the host check parses as an interval"
            );
        }
    }
}
