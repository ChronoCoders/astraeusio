use tokio::sync::{mpsc, oneshot};
use tracing::error;

use crate::{
    db::{Db, DbError},
    iss::IssPosition,
    nasa::{Apod, EpicImage, Exoplanet, NeoFeed},
    noaa::{DstRecord, ImfRecord, Kp3hRecord, KpRecord, SolarWindRecord, SpaceWeatherAlert, XRayRecord},
    starlink::StarlinkSat,
};

pub enum WriteCmd {
    // Poller fire-and-forget
    Iss(IssPosition),
    Kp(Vec<KpRecord>),
    Kp3h(Vec<Kp3hRecord>),
    SolarWind(Vec<SolarWindRecord>),
    Xray(Vec<XRayRecord>),
    Alerts(Vec<SpaceWeatherAlert>),
    Neo(Box<NeoFeed>, i64),
    Epic(Vec<EpicImage>),
    Apod(Apod),
    Exoplanets(Vec<Exoplanet>),
    Imf(Vec<ImfRecord>),
    Dst(Vec<DstRecord>),
    Starlink(Vec<StarlinkSat>),
    Anomaly {
        anomaly_type: String,
        source_ref: String,
        severity: String,
        message: String,
    },
    KpForecast {
        ts: i64,
        kp_e2: i64,
    },
    TouchApiKey(String),
    FlushUsage {
        email: String,
        count: i64,
        period_start: i64,
        period_end: i64,
    },
    // Writes that need a reply
    CreateUser {
        email: String,
        hash: String,
        reply: oneshot::Sender<Result<(), DbError>>,
    },
    UpdatePassword {
        email: String,
        hash: String,
        reply: oneshot::Sender<Result<(), DbError>>,
    },
    CreateApiKey {
        id: String,
        user_email: String,
        key_hash: String,
        name: String,
        reply: oneshot::Sender<Result<(), DbError>>,
    },
    DeleteApiKey {
        id: String,
        user_email: String,
        reply: oneshot::Sender<Result<bool, DbError>>,
    },
}

#[derive(Clone)]
pub struct DbWriterHandle {
    tx: mpsc::Sender<WriteCmd>,
}

impl DbWriterHandle {
    pub fn fire(&self, cmd: WriteCmd) {
        if self.tx.try_send(cmd).is_err() {
            error!("db_writer: queue full or closed, dropping write");
        }
    }

    pub async fn create_user(&self, email: String, hash: String) -> Result<(), DbError> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(WriteCmd::CreateUser { email, hash, reply: tx })
            .await
            .map_err(|_| DbError::WriterClosed)?;
        rx.await.map_err(|_| DbError::WriterClosed)?
    }

    pub async fn update_password(&self, email: String, hash: String) -> Result<(), DbError> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(WriteCmd::UpdatePassword { email, hash, reply: tx })
            .await
            .map_err(|_| DbError::WriterClosed)?;
        rx.await.map_err(|_| DbError::WriterClosed)?
    }

    pub async fn create_api_key(
        &self,
        id: String,
        user_email: String,
        key_hash: String,
        name: String,
    ) -> Result<(), DbError> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(WriteCmd::CreateApiKey { id, user_email, key_hash, name, reply: tx })
            .await
            .map_err(|_| DbError::WriterClosed)?;
        rx.await.map_err(|_| DbError::WriterClosed)?
    }

    pub async fn delete_api_key(&self, id: String, user_email: String) -> Result<bool, DbError> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(WriteCmd::DeleteApiKey { id, user_email, reply: tx })
            .await
            .map_err(|_| DbError::WriterClosed)?;
        rx.await.map_err(|_| DbError::WriterClosed)?
    }
}

pub fn spawn(db: Db) -> DbWriterHandle {
    let (tx, rx) = mpsc::channel(1024);
    tokio::spawn(run(db, rx));
    DbWriterHandle { tx }
}

async fn run(db: Db, mut rx: mpsc::Receiver<WriteCmd>) {
    while let Some(cmd) = rx.recv().await {
        tokio::task::block_in_place(|| process(&db, cmd));
    }
}

fn process(db: &Db, cmd: WriteCmd) {
    match cmd {
        WriteCmd::Iss(pos) => {
            if let Err(e) = db.insert_iss_position(&pos) {
                error!(source = "db_writer", "iss: {e}");
            }
        }
        WriteCmd::Kp(records) => {
            if let Err(e) = db.insert_kp_batch(&records) {
                error!(source = "db_writer", "kp: {e}");
            }
        }
        WriteCmd::Kp3h(records) => {
            if let Err(e) = db.insert_kp_3h_batch(&records) {
                error!(source = "db_writer", "kp-3h: {e}");
            }
        }
        WriteCmd::SolarWind(records) => {
            if let Err(e) = db.insert_solar_wind_batch(&records) {
                error!(source = "db_writer", "solar-wind: {e}");
            }
        }
        WriteCmd::Xray(records) => {
            if let Err(e) = db.insert_xray_batch(&records) {
                error!(source = "db_writer", "xray: {e}");
            }
        }
        WriteCmd::Alerts(alerts) => {
            if let Err(e) = db.insert_alerts_batch(&alerts) {
                error!(source = "db_writer", "alerts: {e}");
            }
        }
        WriteCmd::Neo(feed, fetched_at) => {
            if let Err(e) = db.insert_neo_batch(&feed, fetched_at) {
                error!(source = "db_writer", "neo: {e}");
            }
        }
        WriteCmd::Epic(images) => {
            if let Err(e) = db.insert_epic_batch(&images) {
                error!(source = "db_writer", "epic: {e}");
            }
        }
        WriteCmd::Apod(apod) => {
            if let Err(e) = db.insert_apod(&apod) {
                error!(source = "db_writer", "apod: {e}");
            }
        }
        WriteCmd::Exoplanets(planets) => {
            if let Err(e) = db.insert_exoplanet_batch(&planets) {
                error!(source = "db_writer", "exoplanets: {e}");
            }
        }
        WriteCmd::Imf(records) => {
            if let Err(e) = db.insert_imf_batch(&records) {
                error!(source = "db_writer", "imf: {e}");
            }
        }
        WriteCmd::Dst(records) => {
            if let Err(e) = db.insert_dst_batch(&records) {
                error!(source = "db_writer", "dst: {e}");
            }
        }
        WriteCmd::Starlink(sats) => {
            if let Err(e) = db.insert_starlink_batch(&sats) {
                error!(source = "db_writer", "starlink: {e}");
            }
        }
        WriteCmd::Anomaly { anomaly_type, source_ref, severity, message } => {
            if let Err(e) = db.insert_anomaly(&anomaly_type, &source_ref, &severity, &message) {
                error!(source = "db_writer", "anomaly: {e}");
            }
        }
        WriteCmd::KpForecast { ts, kp_e2 } => {
            if let Err(e) = db.insert_kp_forecast(ts, kp_e2) {
                error!(source = "db_writer", "kp-forecast: {e}");
            }
        }
        WriteCmd::TouchApiKey(hash) => {
            if let Err(e) = db.touch_api_key(&hash) {
                error!(source = "db_writer", "touch-api-key: {e}");
            }
        }
        WriteCmd::FlushUsage { email, count, period_start, period_end } => {
            if let Err(e) = db.upsert_usage_record(&email, count, period_start, period_end) {
                error!(source = "db_writer", "usage-flush {email}: {e}");
            }
        }
        WriteCmd::CreateUser { email, hash, reply } => {
            let _ = reply.send(db.create_user(&email, &hash));
        }
        WriteCmd::UpdatePassword { email, hash, reply } => {
            let _ = reply.send(db.update_password_hash(&email, &hash));
        }
        WriteCmd::CreateApiKey { id, user_email, key_hash, name, reply } => {
            let _ = reply.send(db.create_api_key(&id, &user_email, &key_hash, &name));
        }
        WriteCmd::DeleteApiKey { id, user_email, reply } => {
            let _ = reply.send(db.delete_api_key(&id, &user_email));
        }
    }
}
