use std::sync::Arc;
use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
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
    /// Total attempts per poll, including the first. Clamped to 1..=5, where 1
    /// means no retry. Parsed and then ignored until 2026-08-11, while both
    /// CLAUDE.md and README.md promised three attempts with backoff.
    pub retry_count: u32,
    /// The client's own timeout, needed to bound a single attempt.
    pub http_timeout: u64,
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
    smtp: Option<mailer::MailerConfig>,
    ml_url: String,
) {
    let cfg = PollerConfig::from_env();
    info!(
        retry_count = cfg.retry_count,
        iss = cfg.iss_interval,
        kp = cfg.kp_interval,
        kp_3h = cfg.kp_3h_interval,
        solar_wind = cfg.solar_wind_interval,
        xray = cfg.xray_interval,
        alerts = cfg.alerts_interval,
        neo = cfg.neo_interval,
        epic = cfg.epic_interval,
        apod = cfg.apod_interval,
        exoplanets = cfg.exoplanet_interval,
        imf = cfg.imf_interval,
        dst = cfg.dst_interval,
        starlink = cfg.starlink_interval,
        anomaly = cfg.anomaly_interval,
        forecast = cfg.forecast_interval,
        "poller: intervals loaded"
    );

    // Tier 0 - tiny/read-only, start immediately
    tokio::spawn(poll_iss(
        client.clone(),
        writer.clone(),
        0,
        cfg.policy("poller/iss", cfg.iss_interval),
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
        cfg.policy("poller/kp", cfg.kp_interval),
    ));
    tokio::spawn(poll_alerts(
        client.clone(),
        writer.clone(),
        10,
        cfg.policy("poller/alerts", cfg.alerts_interval),
    ));
    tokio::spawn(poll_neo(
        client.clone(),
        writer.clone(),
        15,
        cfg.policy("poller/neo", cfg.neo_interval),
    ));
    tokio::spawn(poll_epic(
        client.clone(),
        writer.clone(),
        20,
        cfg.policy("poller/epic", cfg.epic_interval),
    ));
    tokio::spawn(poll_apod(
        client.clone(),
        writer.clone(),
        25,
        cfg.policy("poller/apod", cfg.apod_interval),
    ));
    // Tier 2 - large initial inserts (hundreds to thousands of rows), 8-second spacing
    tokio::spawn(poll_kp_3h(
        client.clone(),
        writer.clone(),
        30,
        cfg.policy("poller/kp-3h", cfg.kp_3h_interval),
    ));
    tokio::spawn(poll_dst(
        client.clone(),
        writer.clone(),
        38,
        cfg.policy("poller/dst", cfg.dst_interval),
    ));
    tokio::spawn(poll_exoplanets(
        client.clone(),
        writer.clone(),
        46,
        cfg.policy("poller/exoplanets", cfg.exoplanet_interval),
    ));
    tokio::spawn(poll_imf(
        client.clone(),
        writer.clone(),
        54,
        cfg.policy("poller/imf", cfg.imf_interval),
    ));
    tokio::spawn(poll_solar_wind(
        client.clone(),
        writer.clone(),
        62,
        cfg.policy("poller/solar-wind", cfg.solar_wind_interval),
    ));
    tokio::spawn(poll_xray(
        client.clone(),
        writer.clone(),
        70,
        cfg.policy("poller/xray", cfg.xray_interval),
    ));
    // Tier 3 - Starlink: DELETE + 7000+ inserts in one transaction, start last
    tokio::spawn(poll_starlink(
        client.clone(),
        writer.clone(),
        90,
        cfg.policy("poller/starlink", cfg.starlink_interval),
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
        cfg.policy("poller/forecast", cfg.forecast_interval),
    ));
    // Health snapshots - record per-component status every 5 minutes for the
    // status page's 90-day uptime strip.
    tokio::spawn(poll_health_snapshots(
        client.clone(),
        db.clone(),
        writer.clone(),
        ml_url,
        60,
        300,
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

async fn poll_alerts(
    client: reqwest::Client,
    writer: DbWriterHandle,
    init_delay_secs: u64,
    policy: retry::Policy,
) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        if let Some(fetched) = retry::run(&policy, || noaa::fetch_alerts(&client)).await {
            log_poll("poller/alerts", "alerts", fetched.outcome);
            writer.fire(WriteCmd::Alerts(fetched.items));
        }
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
                        Ok(payload) => {
                            if let Some(kp) =
                                payload.get("predicted_kp").and_then(|v| v.as_f64())
                            {
                                let forecast_ts = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs()
                                    as i64
                                    + 3 * 3600;
                                let ci_l = payload
                                    .get("ci_lower")
                                    .and_then(|v| v.as_f64())
                                    .map(|v| (v * 100.0).round() as i64);
                                let ci_u = payload
                                    .get("ci_upper")
                                    .and_then(|v| v.as_f64())
                                    .map(|v| (v * 100.0).round() as i64);
                                let unc = payload
                                    .get("uncertainty")
                                    .and_then(|v| v.as_f64())
                                    .map(|v| (v * 10_000.0).round() as i64);
                                writer.fire(WriteCmd::KpForecast {
                                    ts: forecast_ts,
                                    kp_e2: (kp * 100.0).round() as i64,
                                    ci_lower_e2: ci_l,
                                    ci_upper_e2: ci_u,
                                    uncertainty_e4: unc,
                                });
                                info!("poller/forecast: predicted Kp {kp:.2} @ +3h");
                            }
                        }
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
    smtp: Option<mailer::MailerConfig>,
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
            dispatch_email_alerts(&db, &writer, cfg).await;
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
}

async fn dispatch_email_alerts(
    db: &Arc<Mutex<Store>>,
    writer: &DbWriterHandle,
    cfg: &mailer::MailerConfig,
) {
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

    for sub in subs {
        if let Some(last) = sub.last_notified_at
            && now_ts - last < COOLDOWN_SECS
        {
            continue;
        }

        let mut lines: Vec<String> = Vec::new();

        if let Some((_, kp_e2)) = kp_opt
            && kp_e2 >= sub.kp_threshold_e2
        {
            let kp = kp_e2 as f64 / 100.0;
            let thr = sub.kp_threshold_e2 as f64 / 100.0;
            lines.push(format!("• Kp index {kp:.1} (your threshold: {thr:.1})"));
        }

        if let Some((_, speed_e1)) = wind_opt
            && speed_e1 >= sub.wind_threshold_e1
        {
            let speed = speed_e1 as f64 / 10.0;
            let thr = sub.wind_threshold_e1 as f64 / 10.0;
            lines.push(format!(
                "• Solar wind {speed:.0} km/s (your threshold: {thr:.0} km/s)"
            ));
        }

        if lines.is_empty() {
            continue;
        }

        writer.fire(WriteCmd::TouchEmailAlertNotified(sub.user_email.clone()));

        let email = sub.user_email.clone();
        let cfg = cfg.clone();
        let body = format!(
            "Space Weather Alert\n\nThe following conditions have exceeded your thresholds:\n\n{}\n\nView your dashboard: https://astraeusio.com\n\nTo update alert settings, visit the API Keys page in your dashboard.",
            lines.join("\n")
        );
        tokio::spawn(async move {
            mailer::send_alert_email(&cfg, &email, "Astraeusio Space Weather Alert", &body).await;
        });
    }
}

// ── Health snapshot poller ────────────────────────────────────────────────────

async fn poll_health_snapshots(
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
        let db_status = if series.iter().any(|(_, _, ts)| ts.is_some()) {
            "operational"
        } else {
            "unknown"
        };

        // Each NOAA series records its own history, so a feed that stops shows
        // as its own gap on the status page instead of being averaged away.
        for (component, status, _) in &series {
            writer.fire(WriteCmd::HealthSnapshot {
                component: (*component).to_string(),
                ts: now,
                status: (*status).to_string(),
            });
        }

        for (component, status) in [
            ("backend_api", "operational"),
            ("ml_forecast", ml_status),
            ("database", db_status),
            ("celestrak", celestrak_status),
        ] {
            writer.fire(WriteCmd::HealthSnapshot {
                component: component.to_string(),
                ts: now,
                status: status.to_string(),
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
