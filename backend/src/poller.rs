use std::sync::Arc;
use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use tokio::sync::Mutex;
use tracing::{error, info};

use crate::{db::Db, iss, nasa, noaa};

pub fn spawn(client: reqwest::Client, db: Arc<Mutex<Db>>) {
    tokio::spawn(poll_iss(client.clone(), db.clone()));
    tokio::spawn(poll_kp(client.clone(), db.clone()));
    tokio::spawn(poll_solar_wind(client.clone(), db.clone()));
    tokio::spawn(poll_xray(client.clone(), db.clone()));
    tokio::spawn(poll_alerts(client.clone(), db.clone()));
    tokio::spawn(poll_neo(client.clone(), db.clone()));
    tokio::spawn(poll_epic(client.clone(), db.clone()));
    tokio::spawn(poll_apod(client.clone(), db.clone()));
    tokio::spawn(poll_exoplanets(client.clone(), db.clone()));
}

async fn poll_iss(client: reqwest::Client, db: Arc<Mutex<Db>>) {
    loop {
        match iss::fetch_iss_position(&client).await {
            Ok(pos) => {
                info!("poller/iss: lat={:.4} lon={:.4}", pos.latitude, pos.longitude);
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
                let db = db.lock().await;
                for r in &records {
                    if let Err(e) = db.insert_kp(r) {
                        error!(source = "poller/kp", "insert: {e}");
                    }
                }
            }
            Err(e) => error!(source = "poller/kp", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

async fn poll_solar_wind(client: reqwest::Client, db: Arc<Mutex<Db>>) {
    loop {
        match noaa::fetch_solar_wind(&client).await {
            Ok(records) => {
                info!("poller/solar-wind: {} records", records.len());
                let db = db.lock().await;
                if let Err(e) = (|| -> Result<(), crate::db::DbError> {
                    db.begin()?;
                    let result = records.iter().try_for_each(|r| db.insert_solar_wind(r));
                    match result {
                        Ok(()) => db.commit(),
                        Err(e) => { db.rollback(); Err(e) }
                    }
                })() {
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
                let db = db.lock().await;
                if let Err(e) = (|| -> Result<(), crate::db::DbError> {
                    db.begin()?;
                    let result = records.iter().try_for_each(|r| db.insert_xray(r));
                    match result {
                        Ok(()) => db.commit(),
                        Err(e) => { db.rollback(); Err(e) }
                    }
                })() {
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
                let db = db.lock().await;
                for a in &alerts {
                    if let Err(e) = db.insert_alert(a) {
                        error!(source = "poller/alerts", "insert: {e}");
                    }
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
        let end = (today + ChronoDuration::days(7)).format("%Y-%m-%d").to_string();
        match nasa::fetch_neo_feed(&client, &start, &end).await {
            Ok(feed) => {
                info!("poller/neo: {} objects", feed.element_count);
                let fetched_at = Utc::now().timestamp();
                let db = db.lock().await;
                for neos in feed.near_earth_objects.values() {
                    for neo in neos {
                        for approach in &neo.close_approach_data {
                            if let Err(e) = db.insert_neo(neo, approach, fetched_at) {
                                error!(source = "poller/neo", "insert: {e}");
                            }
                        }
                    }
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
                let db = db.lock().await;
                for img in &images {
                    if let Err(e) = db.insert_epic_image(img) {
                        error!(source = "poller/epic", "insert: {e}");
                    }
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
                let db = db.lock().await;
                for p in &planets {
                    if let Err(e) = db.insert_exoplanet(p) {
                        error!(source = "poller/exoplanets", "insert: {e}");
                    }
                }
            }
            Err(e) => error!(source = "poller/exoplanets", "fetch: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(86400)).await;
    }
}
