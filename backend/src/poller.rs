use std::sync::Arc;
use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use tokio::sync::Mutex;
use tracing::{error, info};

use crate::{anomaly, db::Db, iss, nasa, noaa, starlink};

pub fn spawn(client: reqwest::Client, db: Arc<Mutex<Db>>) {
    tokio::spawn(poll_iss(client.clone(), db.clone()));
    tokio::spawn(poll_kp(client.clone(), db.clone()));
    tokio::spawn(poll_kp_3h(client.clone(), db.clone()));
    tokio::spawn(poll_solar_wind(client.clone(), db.clone()));
    tokio::spawn(poll_xray(client.clone(), db.clone()));
    tokio::spawn(poll_alerts(client.clone(), db.clone()));
    tokio::spawn(poll_neo(client.clone(), db.clone()));
    tokio::spawn(poll_epic(client.clone(), db.clone()));
    tokio::spawn(poll_apod(client.clone(), db.clone()));
    tokio::spawn(poll_exoplanets(client.clone(), db.clone()));
    tokio::spawn(poll_anomaly(db.clone()));
    tokio::spawn(poll_imf(client.clone(), db.clone()));
    tokio::spawn(poll_dst(client.clone(), db.clone()));
    tokio::spawn(poll_starlink(client.clone(), db.clone()));
}

async fn poll_iss(client: reqwest::Client, db: Arc<Mutex<Db>>) {
    loop {
        match iss::fetch_iss_position(&client).await {
            Ok(pos) => {
                info!(
                    "poller/iss: lat={:.4} lon={:.4}",
                    pos.latitude, pos.longitude
                );
                if let Err(e) = db.lock().await.insert_iss_position(&pos) {
                    error!(source = "poller/iss", "insert: {e}");
                }
            }
            Err(e) => error!(source = "poller/iss", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

async fn poll_kp(client: reqwest::Client, db: Arc<Mutex<Db>>) {
    loop {
        match noaa::fetch_kp(&client).await {
            Ok(records) => {
                info!("poller/kp: {} records", records.len());
                if let Err(e) = db.lock().await.insert_kp_batch(&records) {
                    error!(source = "poller/kp", "insert: {e}");
                }
            }
            Err(e) => error!(source = "poller/kp", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

async fn poll_kp_3h(client: reqwest::Client, db: Arc<Mutex<Db>>) {
    loop {
        match noaa::fetch_kp_3h(&client).await {
            Ok(records) => {
                info!("poller/kp-3h: {} records", records.len());
                if let Err(e) = db.lock().await.insert_kp_3h_batch(&records) {
                    error!(source = "poller/kp-3h", "insert: {e}");
                }
            }
            Err(e) => error!(source = "poller/kp-3h", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(1800)).await;
    }
}

async fn poll_solar_wind(client: reqwest::Client, db: Arc<Mutex<Db>>) {
    loop {
        match noaa::fetch_solar_wind(&client).await {
            Ok(records) => {
                info!("poller/solar-wind: {} records", records.len());
                if let Err(e) = db.lock().await.insert_solar_wind_batch(&records) {
                    error!(source = "poller/solar-wind", "insert: {e}");
                }
            }
            Err(e) => error!(source = "poller/solar-wind", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

async fn poll_xray(client: reqwest::Client, db: Arc<Mutex<Db>>) {
    loop {
        match noaa::fetch_xray(&client).await {
            Ok(records) => {
                info!("poller/xray: {} records", records.len());
                if let Err(e) = db.lock().await.insert_xray_batch(&records) {
                    error!(source = "poller/xray", "insert: {e}");
                }
            }
            Err(e) => error!(source = "poller/xray", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(120)).await;
    }
}

async fn poll_alerts(client: reqwest::Client, db: Arc<Mutex<Db>>) {
    loop {
        match noaa::fetch_alerts(&client).await {
            Ok(alerts) => {
                info!("poller/alerts: {}", alerts.len());
                if let Err(e) = db.lock().await.insert_alerts_batch(&alerts) {
                    error!(source = "poller/alerts", "insert: {e}");
                }
            }
            Err(e) => error!(source = "poller/alerts", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(300)).await;
    }
}

async fn poll_neo(client: reqwest::Client, db: Arc<Mutex<Db>>) {
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
                if let Err(e) = db.lock().await.insert_neo_batch(&feed, fetched_at) {
                    error!(source = "poller/neo", "insert: {e}");
                }
            }
            Err(e) => error!(source = "poller/neo", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(1800)).await;
    }
}

async fn poll_epic(client: reqwest::Client, db: Arc<Mutex<Db>>) {
    loop {
        match nasa::fetch_epic(&client).await {
            Ok(images) => {
                info!("poller/epic: {} images", images.len());
                if let Err(e) = db.lock().await.insert_epic_batch(&images) {
                    error!(source = "poller/epic", "insert: {e}");
                }
            }
            Err(e) => error!(source = "poller/epic", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(1800)).await;
    }
}

async fn poll_apod(client: reqwest::Client, db: Arc<Mutex<Db>>) {
    loop {
        match nasa::fetch_apod(&client).await {
            Ok(apod) => {
                info!("poller/apod: {}", apod.date);
                if let Err(e) = db.lock().await.insert_apod(&apod) {
                    error!(source = "poller/apod", "insert: {e}");
                }
            }
            Err(e) => error!(source = "poller/apod", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(3600)).await;
    }
}

async fn poll_exoplanets(client: reqwest::Client, db: Arc<Mutex<Db>>) {
    loop {
        match nasa::fetch_exoplanets(&client).await {
            Ok(planets) => {
                info!("poller/exoplanets: {}", planets.len());
                if let Err(e) = db.lock().await.insert_exoplanet_batch(&planets) {
                    error!(source = "poller/exoplanets", "insert: {e}");
                }
            }
            Err(e) => error!(source = "poller/exoplanets", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(86400)).await;
    }
}

async fn poll_imf(client: reqwest::Client, db: Arc<Mutex<Db>>) {
    loop {
        match noaa::fetch_imf(&client).await {
            Ok(records) => {
                info!("poller/imf: {} records", records.len());
                if let Err(e) = db.lock().await.insert_imf_batch(&records) {
                    error!(source = "poller/imf", "insert: {e}");
                }
            }
            Err(e) => error!(source = "poller/imf", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

async fn poll_dst(client: reqwest::Client, db: Arc<Mutex<Db>>) {
    loop {
        match noaa::fetch_dst(&client).await {
            Ok(records) => {
                info!("poller/dst: {} records", records.len());
                if let Err(e) = db.lock().await.insert_dst_batch(&records) {
                    error!(source = "poller/dst", "insert: {e}");
                }
            }
            Err(e) => error!(source = "poller/dst", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(300)).await;
    }
}

async fn poll_starlink(client: reqwest::Client, db: Arc<Mutex<Db>>) {
    loop {
        match starlink::fetch_starlink(&client).await {
            Ok(sats) => {
                info!("poller/starlink: {} satellites", sats.len());
                if let Err(e) = db.lock().await.insert_starlink_batch(&sats) {
                    error!(source = "poller/starlink", "insert: {e}");
                }
            }
            Err(e) => error!(source = "poller/starlink", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(3600)).await;
    }
}

async fn poll_anomaly(db: Arc<Mutex<Db>>) {
    loop {
        {
            let db_guard = db.lock().await;
            if let Err(e) = anomaly::detect_and_store(&db_guard) {
                error!(source = "poller/anomaly", "detect: {e}");
            }
        }
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}
