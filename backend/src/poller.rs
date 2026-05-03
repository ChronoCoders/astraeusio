use std::sync::Arc;
use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use tokio::sync::Mutex;
use tracing::{error, info};

use crate::{
    anomaly,
    db::Store,
    db_writer::{DbWriterHandle, WriteCmd},
    iss, mailer, nasa, noaa, starlink,
};

// ── Poller configuration ──────────────────────────────────────────────────────

pub struct PollerConfig {
    pub iss_interval:        u64,
    pub kp_interval:         u64,
    pub kp_3h_interval:      u64,
    pub solar_wind_interval: u64,
    pub xray_interval:       u64,
    pub alerts_interval:     u64,
    pub neo_interval:        u64,
    pub epic_interval:       u64,
    pub apod_interval:       u64,
    pub exoplanet_interval:  u64,
    pub imf_interval:        u64,
    pub dst_interval:        u64,
    pub starlink_interval:   u64,
    pub anomaly_interval:    u64,
    pub retry_count:         u32,
}

impl PollerConfig {
    pub fn from_env() -> Self {
        fn secs(key: &str, default: u64) -> u64 {
            std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
        }
        Self {
            iss_interval:        secs("ISS_INTERVAL",        5),
            kp_interval:         secs("KP_INTERVAL",         60),
            kp_3h_interval:      secs("KP_3H_INTERVAL",      1800),
            solar_wind_interval: secs("SOLAR_WIND_INTERVAL", 60),
            xray_interval:       secs("XRAY_INTERVAL",       120),
            alerts_interval:     secs("ALERTS_INTERVAL",     300),
            neo_interval:        secs("NEO_INTERVAL",        1800),
            epic_interval:       secs("EPIC_INTERVAL",       1800),
            apod_interval:       secs("APOD_INTERVAL",       3600),
            exoplanet_interval:  secs("EXOPLANET_INTERVAL",  86400),
            imf_interval:        secs("IMF_INTERVAL",        60),
            dst_interval:        secs("DST_INTERVAL",        300),
            starlink_interval:   secs("STARLINK_INTERVAL",   3600),
            anomaly_interval:    secs("ANOMALY_INTERVAL",    60),
            retry_count:         std::env::var("RETRY_COUNT")
                .ok().and_then(|v| v.parse().ok()).unwrap_or(3),
        }
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
) {
    let cfg = PollerConfig::from_env();
    info!(
        retry_count = cfg.retry_count,
        iss = cfg.iss_interval, kp = cfg.kp_interval, kp_3h = cfg.kp_3h_interval,
        solar_wind = cfg.solar_wind_interval, xray = cfg.xray_interval,
        alerts = cfg.alerts_interval, neo = cfg.neo_interval,
        epic = cfg.epic_interval, apod = cfg.apod_interval,
        exoplanets = cfg.exoplanet_interval, imf = cfg.imf_interval,
        dst = cfg.dst_interval, starlink = cfg.starlink_interval,
        anomaly = cfg.anomaly_interval,
        "poller: intervals loaded"
    );

    // Tier 0 — tiny/read-only, start immediately
    tokio::spawn(poll_iss(client.clone(), writer.clone(), 0, cfg.iss_interval));
    tokio::spawn(poll_anomaly(db.clone(), writer.clone(), smtp, 2, cfg.anomaly_interval));
    // Tier 1 — small inserts, 5-second spacing
    tokio::spawn(poll_kp(client.clone(), writer.clone(), 5, cfg.kp_interval));
    tokio::spawn(poll_alerts(client.clone(), writer.clone(), 10, cfg.alerts_interval));
    tokio::spawn(poll_neo(client.clone(), writer.clone(), 15, cfg.neo_interval));
    tokio::spawn(poll_epic(client.clone(), writer.clone(), 20, cfg.epic_interval));
    tokio::spawn(poll_apod(client.clone(), writer.clone(), 25, cfg.apod_interval));
    // Tier 2 — large initial inserts (hundreds to thousands of rows), 8-second spacing
    tokio::spawn(poll_kp_3h(client.clone(), writer.clone(), 30, cfg.kp_3h_interval));
    tokio::spawn(poll_dst(client.clone(), writer.clone(), 38, cfg.dst_interval));
    tokio::spawn(poll_exoplanets(client.clone(), writer.clone(), 46, cfg.exoplanet_interval));
    tokio::spawn(poll_imf(client.clone(), writer.clone(), 54, cfg.imf_interval));
    tokio::spawn(poll_solar_wind(client.clone(), writer.clone(), 62, cfg.solar_wind_interval));
    tokio::spawn(poll_xray(client.clone(), writer.clone(), 70, cfg.xray_interval));
    // Tier 3 — Starlink: DELETE + 7000+ inserts in one transaction, start last
    tokio::spawn(poll_starlink(client.clone(), writer.clone(), 90, cfg.starlink_interval));
}

// ── Poll functions ────────────────────────────────────────────────────────────

async fn poll_iss(client: reqwest::Client, writer: DbWriterHandle, init_delay_secs: u64, interval: u64) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        match iss::fetch_iss_position(&client).await {
            Ok(pos) => {
                info!("poller/iss: lat={:.4} lon={:.4}", pos.latitude, pos.longitude);
                writer.fire(WriteCmd::Iss(pos));
            }
            Err(e) => error!(source = "poller/iss", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
}

async fn poll_kp(client: reqwest::Client, writer: DbWriterHandle, init_delay_secs: u64, interval: u64) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        match noaa::fetch_kp(&client).await {
            Ok(records) => {
                info!("poller/kp: {} records", records.len());
                writer.fire(WriteCmd::Kp(records));
            }
            Err(e) => error!(source = "poller/kp", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
}

async fn poll_kp_3h(client: reqwest::Client, writer: DbWriterHandle, init_delay_secs: u64, interval: u64) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        match noaa::fetch_kp_3h(&client).await {
            Ok(records) => {
                info!("poller/kp-3h: {} records", records.len());
                writer.fire(WriteCmd::Kp3h(records));
            }
            Err(e) => error!(source = "poller/kp-3h", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
}

async fn poll_solar_wind(client: reqwest::Client, writer: DbWriterHandle, init_delay_secs: u64, interval: u64) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        match noaa::fetch_solar_wind(&client).await {
            Ok(records) => {
                info!("poller/solar-wind: {} records", records.len());
                writer.fire(WriteCmd::SolarWind(records));
            }
            Err(e) => error!(source = "poller/solar-wind", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
}

async fn poll_xray(client: reqwest::Client, writer: DbWriterHandle, init_delay_secs: u64, interval: u64) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        match noaa::fetch_xray(&client).await {
            Ok(records) => {
                info!("poller/xray: {} records", records.len());
                writer.fire(WriteCmd::Xray(records));
            }
            Err(e) => error!(source = "poller/xray", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
}

async fn poll_alerts(client: reqwest::Client, writer: DbWriterHandle, init_delay_secs: u64, interval: u64) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        match noaa::fetch_alerts(&client).await {
            Ok(alerts) => {
                info!("poller/alerts: {}", alerts.len());
                writer.fire(WriteCmd::Alerts(alerts));
            }
            Err(e) => error!(source = "poller/alerts", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
}

async fn poll_neo(client: reqwest::Client, writer: DbWriterHandle, init_delay_secs: u64, interval: u64) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        let today = Utc::now().date_naive();
        let start = today.format("%Y-%m-%d").to_string();
        let end = (today + ChronoDuration::days(7)).format("%Y-%m-%d").to_string();
        match nasa::fetch_neo_feed(&client, &start, &end).await {
            Ok(feed) => {
                info!("poller/neo: {} objects", feed.element_count);
                let fetched_at = Utc::now().timestamp();
                writer.fire(WriteCmd::Neo(Box::new(feed), fetched_at));
            }
            Err(e) => error!(source = "poller/neo", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
}

async fn poll_epic(client: reqwest::Client, writer: DbWriterHandle, init_delay_secs: u64, interval: u64) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        match nasa::fetch_epic(&client).await {
            Ok(images) => {
                info!("poller/epic: {} images", images.len());
                writer.fire(WriteCmd::Epic(images));
            }
            Err(e) => error!(source = "poller/epic", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
}

async fn poll_apod(client: reqwest::Client, writer: DbWriterHandle, init_delay_secs: u64, interval: u64) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        match nasa::fetch_apod(&client).await {
            Ok(apod) => {
                info!("poller/apod: {}", apod.date);
                writer.fire(WriteCmd::Apod(apod));
            }
            Err(e) => error!(source = "poller/apod", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
}

async fn poll_exoplanets(client: reqwest::Client, writer: DbWriterHandle, init_delay_secs: u64, interval: u64) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        match nasa::fetch_exoplanets(&client).await {
            Ok(planets) => {
                info!("poller/exoplanets: {}", planets.len());
                writer.fire(WriteCmd::Exoplanets(planets));
            }
            Err(e) => error!(source = "poller/exoplanets", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
}

async fn poll_imf(client: reqwest::Client, writer: DbWriterHandle, init_delay_secs: u64, interval: u64) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        match noaa::fetch_imf(&client).await {
            Ok(records) => {
                info!("poller/imf: {} records", records.len());
                writer.fire(WriteCmd::Imf(records));
            }
            Err(e) => error!(source = "poller/imf", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
}

async fn poll_dst(client: reqwest::Client, writer: DbWriterHandle, init_delay_secs: u64, interval: u64) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        match noaa::fetch_dst(&client).await {
            Ok(records) => {
                info!("poller/dst: {} records", records.len());
                writer.fire(WriteCmd::Dst(records));
            }
            Err(e) => error!(source = "poller/dst", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
}

async fn poll_starlink(client: reqwest::Client, writer: DbWriterHandle, init_delay_secs: u64, interval: u64) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        match starlink::fetch_starlink(&client).await {
            Ok(sats) => {
                info!("poller/starlink: {} satellites", sats.len());
                writer.fire(WriteCmd::Starlink(sats));
            }
            Err(e) => error!(source = "poller/starlink", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
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
            lines.push(format!("• Solar wind {speed:.0} km/s (your threshold: {thr:.0} km/s)"));
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
