mod anomaly;
mod auth;
mod db;
mod iss;
mod nasa;
mod noaa;
mod poller;
mod routes;
mod starlink;

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
    let db = db::Db::open(&db_path)?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()?;
    let ml_url =
        std::env::var("ML_SERVICE_URL").unwrap_or_else(|_| "http://localhost:8000".to_string());
    let jwt_secret = std::env::var("JWT_SECRET").expect("JWT_SECRET must be set");
    let state = routes::AppState::new(client, db, ml_url, jwt_secret);

    poller::spawn(state.client.clone(), state.db.clone());

    let app = routes::router(state);

    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".to_string());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("listening on {addr}");

    axum::serve(listener, app).await?;

    Ok(())
}
