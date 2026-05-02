mod anomaly;
mod api_keys;
mod auth;
mod db;
mod db_writer;
mod iss;
mod nasa;
mod noaa;
mod plan;
mod poller;
mod rate_limit;
mod routes;
mod starlink;
mod webhook_sender;
mod webhooks;

use anyhow::Result;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let db_path = std::env::var("DB_PATH").unwrap_or_else(|_| "astraeus.duckdb".to_string());
    let write_db = db::Db::open(&db_path)?;
    let read_db = write_db.try_clone()?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()?;
    let writer = db_writer::spawn(write_db, client.clone());
    let ml_url =
        std::env::var("ML_SERVICE_URL").unwrap_or_else(|_| "http://localhost:8000".to_string());
    let jwt_secret = std::env::var("JWT_SECRET").expect("JWT_SECRET must be set");
    let state = routes::AppState::new(client, read_db, writer.clone(), ml_url, jwt_secret);

    poller::spawn(state.client.clone(), state.db.clone(), writer.clone());
    rate_limit::spawn_flush_task(state.usage_counter.clone(), writer);

    let app = routes::router(state);

    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".to_string());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("listening on {addr}");

    axum::serve(listener, app).await?;

    Ok(())
}
