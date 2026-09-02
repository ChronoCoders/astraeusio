//! Astraeusio backend.

// No unsafe in the shipped binary, enforced by the compiler rather than by
// review. `forbid` and not `deny` because `forbid` cannot be switched off by an
// inner `allow`, which is the point: a future lifetime problem should not be
// solvable by writing `#[allow(unsafe_code)]` above the function that has it.
//
// The `not(test)` is a real exemption and it is deliberate. Eleven sites in the
// test modules call `std::env::set_var` and `remove_var` on
// `TOTP_ENCRYPTION_KEY` and `ALLOW_SELF_SERVE_PLAN_CHANGE`, which Edition 2024
// reclassified as unsafe because the process environment is not thread safe.
// Nobody wrote unsafe code; an edition bump made existing test setup unsafe.
// A plain `#![forbid(unsafe_code)]` therefore does not compile, and the honest
// choice is a narrower guarantee stated accurately. The exemption is smaller
// than it looks: `cargo test` compiles this crate twice, once with `cfg(test)`
// for the unit test harness and once without it for the binary, so unsafe
// written in ordinary code is still rejected by `cargo test` as well as by
// `cargo build`. What the exemption permits is unsafe inside `#[cfg(test)]`
// code, which exists only in the compilation where the attribute is absent.
// Verified by reintroducing an unsafe block in ordinary code: both `cargo build`
// and `cargo test --no-run` refuse it with "usage of an `unsafe` block".
//
// Closing the gap means removing those eleven sites rather than weakening this
// line. Each of them exists because a function reads its configuration from the
// environment directly, so a test can only steer it by mutating the process. The
// fix is the shape `health_interval_secs` already has: the logic takes the value
// as an argument and a thin wrapper reads the environment, after which tests
// pass a value and touch no globals. That is three test modules and their
// callers, so it waits until something else is touching them.

#![cfg_attr(not(test), forbid(unsafe_code))]

mod anomaly;
mod api_keys;
mod astros;
mod auth;
mod db;
mod db_writer;
mod email_alerts;
mod fetch;
mod iss;
mod mailer;
mod nasa;
mod noaa;
mod oauth;
mod plan;
mod poller;
mod rate_limit;
mod redact;
mod retry;
mod routes;
mod secretbox;
mod starlink;
mod webhook_guard;
mod webhook_sender;
mod webhooks;

use anyhow::Result;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let db_path = std::env::var("DB_PATH").unwrap_or_else(|_| "astraeus.duckdb".to_string());
    let write_db = db::Store::open(&db_path)?;
    let read_db = write_db.try_clone()?;
    let http_timeout = std::env::var("HTTP_TIMEOUT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60u64);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(http_timeout))
        .build()?;
    // Webhook delivery goes out on its own client: https only, no redirects,
    // and a resolver that refuses any answer containing a non-public address
    // (AUD-004). `db_writer` uses the client it is handed for webhook delivery
    // and for nothing else, which is why this one is the one it gets.
    let webhook_client = webhook_guard::client(std::time::Duration::from_secs(10))?;
    let writer = db_writer::spawn(write_db, webhook_client);

    // A rule change that silently orphans live integrations is the failure this
    // audit keeps finding. Syntax only, so startup never waits on DNS.
    match read_db.list_webhook_targets() {
        Ok(targets) => {
            let refused: Vec<String> = targets
                .iter()
                .filter_map(|(id, url)| {
                    webhook_guard::validate_syntax(url)
                        .err()
                        .map(|r| format!("{id} ({r})"))
                })
                .collect();
            if refused.is_empty() {
                info!("webhooks: {} stored, all deliverable", targets.len());
            } else {
                warn!(
                    "webhooks: {} of {} stored targets are refused by the delivery rules and will                      not be sent: {}",
                    refused.len(),
                    targets.len(),
                    refused.join(", ")
                );
            }
        }
        Err(e) => warn!("webhooks: could not scan stored targets: {e}"),
    }
    let ml_url =
        std::env::var("ML_SERVICE_URL").unwrap_or_else(|_| "http://localhost:8000".to_string());
    let jwt_secret = std::env::var("JWT_SECRET").expect("JWT_SECRET must be set");
    let mailer_config = mailer::MailerConfig::from_env();
    let app_url = std::env::var("APP_URL").unwrap_or_else(|_| "http://localhost:5173".to_string());
    let oauth_config = oauth::OAuthConfig::from_env(&app_url);
    info!("oauth providers enabled: {:?}", oauth_config.enabled());
    let state = routes::AppState::new(
        client,
        read_db,
        writer.clone(),
        ml_url,
        jwt_secret,
        mailer_config.clone(),
        app_url,
        oauth_config,
    );

    poller::spawn(
        state.client.clone(),
        state.db.clone(),
        writer.clone(),
        mailer_config,
        state.ml_url.clone(),
    );
    rate_limit::spawn_flush_task(state.usage_counter.clone(), writer);

    let app = routes::router(state);

    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".to_string());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("listening on {addr}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("shutdown complete");
    Ok(())
}

async fn shutdown_signal() {
    use tokio::signal;

    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c   => info!("received Ctrl+C, shutting down"),
        _ = terminate => info!("received SIGTERM, shutting down"),
    }
}
