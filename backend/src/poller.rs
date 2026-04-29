use std::sync::Arc;
use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use tokio::sync::Mutex;
use tracing::{error, info};

use crate::{anomaly, db::Db, db_writer::{DbWriterHandle, WriteCmd}, iss, nasa, noaa, starlink};

// Stagger initial poller startup to prevent DB mutex contention on first run.
// Each poller's first insert can be large (thousands of rows); if all fire at
// once the HTTP server is starved for 60+ seconds before any request lands.
pub fn spawn(client: reqwest::Client, db: Arc<Mutex<Db>>, writer: DbWriterHandle) {
    // Tier 0 — tiny/read-only, start immediately
    tokio::spawn(poll_iss(client.clone(), writer.clone(), 0));
    tokio::spawn(poll_anomaly(db.clone(), writer.clone(), 2));
    // Tier 1 — small inserts, 5-second spacing
    tokio::spawn(poll_kp(client.clone(), writer.clone(), 5));
    tokio::spawn(poll_alerts(client.clone(), writer.clone(), 10));
    tokio::spawn(poll_neo(client.clone(), writer.clone(), 15));
    tokio::spawn(poll_epic(client.clone(), writer.clone(), 20));
    tokio::spawn(poll_apod(client.clone(), writer.clone(), 25));
    // Tier 2 — large initial inserts (hundreds to thousands of rows), 8-second spacing
    tokio::spawn(poll_kp_3h(client.clone(), writer.clone(), 30));
    tokio::spawn(poll_dst(client.clone(), writer.clone(), 38));
    tokio::spawn(poll_exoplanets(client.clone(), writer.clone(), 46));
    tokio::spawn(poll_imf(client.clone(), writer.clone(), 54));
    tokio::spawn(poll_solar_wind(client.clone(), writer.clone(), 62));
    tokio::spawn(poll_xray(client.clone(), writer.clone(), 70));
    // Tier 3 — Starlink: DELETE + 7000+ inserts in one transaction, start last
    tokio::spawn(poll_starlink(client.clone(), writer.clone(), 90));
}

async fn poll_iss(client: reqwest::Client, writer: DbWriterHandle, init_delay_secs: u64) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        match iss::fetch_iss_position(&client).await {
            Ok(pos) => {
                info!(
                    "poller/iss: lat={:.4} lon={:.4}",
                    pos.latitude, pos.longitude
                );
                writer.fire(WriteCmd::Iss(pos));
            }
            Err(e) => error!(source = "poller/iss", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

async fn poll_kp(client: reqwest::Client, writer: DbWriterHandle, init_delay_secs: u64) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        match noaa::fetch_kp(&client).await {
            Ok(records) => {
                info!("poller/kp: {} records", records.len());
                writer.fire(WriteCmd::Kp(records));
            }
            Err(e) => error!(source = "poller/kp", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

async fn poll_kp_3h(client: reqwest::Client, writer: DbWriterHandle, init_delay_secs: u64) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        match noaa::fetch_kp_3h(&client).await {
            Ok(records) => {
                info!("poller/kp-3h: {} records", records.len());
                writer.fire(WriteCmd::Kp3h(records));
            }
            Err(e) => error!(source = "poller/kp-3h", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(1800)).await;
    }
}

async fn poll_solar_wind(client: reqwest::Client, writer: DbWriterHandle, init_delay_secs: u64) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        match noaa::fetch_solar_wind(&client).await {
            Ok(records) => {
                info!("poller/solar-wind: {} records", records.len());
                writer.fire(WriteCmd::SolarWind(records));
            }
            Err(e) => error!(source = "poller/solar-wind", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

async fn poll_xray(client: reqwest::Client, writer: DbWriterHandle, init_delay_secs: u64) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        match noaa::fetch_xray(&client).await {
            Ok(records) => {
                info!("poller/xray: {} records", records.len());
                writer.fire(WriteCmd::Xray(records));
            }
            Err(e) => error!(source = "poller/xray", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(120)).await;
    }
}

async fn poll_alerts(client: reqwest::Client, writer: DbWriterHandle, init_delay_secs: u64) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        match noaa::fetch_alerts(&client).await {
            Ok(alerts) => {
                info!("poller/alerts: {}", alerts.len());
                writer.fire(WriteCmd::Alerts(alerts));
            }
            Err(e) => error!(source = "poller/alerts", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(300)).await;
    }
}

async fn poll_neo(client: reqwest::Client, writer: DbWriterHandle, init_delay_secs: u64) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        let today = Utc::now().date_naive();
        let start = today.format("%Y-%m-%d").to_string();
        let end = (today + ChronoDuration::days(7))
            .format("%Y-%m-%d")
            .to_string();
        match nasa::fetch_neo_feed(&client, &start, &end).await {
            Ok(feed) => {
                info!("poller/neo: {} objects", feed.element_count);
                let fetched_at = Utc::now().timestamp();
                writer.fire(WriteCmd::Neo(Box::new(feed), fetched_at));
            }
            Err(e) => error!(source = "poller/neo", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(1800)).await;
    }
}

async fn poll_epic(client: reqwest::Client, writer: DbWriterHandle, init_delay_secs: u64) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        match nasa::fetch_epic(&client).await {
            Ok(images) => {
                info!("poller/epic: {} images", images.len());
                writer.fire(WriteCmd::Epic(images));
            }
            Err(e) => error!(source = "poller/epic", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(1800)).await;
    }
}

async fn poll_apod(client: reqwest::Client, writer: DbWriterHandle, init_delay_secs: u64) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        match nasa::fetch_apod(&client).await {
            Ok(apod) => {
                info!("poller/apod: {}", apod.date);
                writer.fire(WriteCmd::Apod(apod));
            }
            Err(e) => error!(source = "poller/apod", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(3600)).await;
    }
}

async fn poll_exoplanets(client: reqwest::Client, writer: DbWriterHandle, init_delay_secs: u64) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        match nasa::fetch_exoplanets(&client).await {
            Ok(planets) => {
                info!("poller/exoplanets: {}", planets.len());
                writer.fire(WriteCmd::Exoplanets(planets));
            }
            Err(e) => error!(source = "poller/exoplanets", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(86400)).await;
    }
}

async fn poll_imf(client: reqwest::Client, writer: DbWriterHandle, init_delay_secs: u64) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        match noaa::fetch_imf(&client).await {
            Ok(records) => {
                info!("poller/imf: {} records", records.len());
                writer.fire(WriteCmd::Imf(records));
            }
            Err(e) => error!(source = "poller/imf", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

async fn poll_dst(client: reqwest::Client, writer: DbWriterHandle, init_delay_secs: u64) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        match noaa::fetch_dst(&client).await {
            Ok(records) => {
                info!("poller/dst: {} records", records.len());
                writer.fire(WriteCmd::Dst(records));
            }
            Err(e) => error!(source = "poller/dst", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(300)).await;
    }
}

async fn poll_starlink(client: reqwest::Client, writer: DbWriterHandle, init_delay_secs: u64) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        match starlink::fetch_starlink(&client).await {
            Ok(sats) => {
                info!("poller/starlink: {} satellites", sats.len());
                writer.fire(WriteCmd::Starlink(sats));
            }
            Err(e) => error!(source = "poller/starlink", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(3600)).await;
    }
}

async fn poll_anomaly(db: Arc<Mutex<Db>>, writer: DbWriterHandle, init_delay_secs: u64) {
    tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
    loop {
        {
            let db_guard = db.lock().await;
            if let Err(e) = anomaly::detect_and_store(&db_guard, &writer) {
                error!(source = "poller/anomaly", "detect: {e}");
            }
        }
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}
