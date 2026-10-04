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

/// What every outbound request says we are.
///
/// reqwest sends no User-Agent unless one is set, so until 2026-10-04 every
/// poller fetch reached NOAA, NASA, Celestrak, the Exoplanet Archive,
/// wheretheiss.at and Launch Library 2 with no identification at all. For a
/// free service run by one person that is the difference between being able to
/// see who is responsible for the traffic and not. GitHub requires the header
/// and OAuth has always set it per request, which is where this constant lived
/// and why it already existed.
///
/// Deliberately not applied to Resend or webhook delivery: both build their
/// own clients, the first because it is a vendor SDK path and the second
/// because `webhook_guard` constrains it for safety reasons that have nothing
/// to do with identification.
pub const USER_AGENT: &str = "astraeusio";

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
    // Before anything else that can fail slowly. A malformed tool manifest is a
    // programming error in a constant, so it should stop the process here rather
    // than surface as a dead route after a successful looking start.
    routes::init_mcp_tools()?;

    let write_db = db::Store::open(&db_path)?;
    let read_db = write_db.try_clone()?;
    let http_timeout = std::env::var("HTTP_TIMEOUT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60u64);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(http_timeout))
        .user_agent(USER_AGENT)
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
    // One `Sender` for the whole process, behind the trait so tests can put a
    // different one in. `None` when no key is configured, exactly as before.
    let mailer_config: Option<std::sync::Arc<dyn mailer::Sender>> =
        mailer::MailerConfig::from_env().map(|c| {
            std::sync::Arc::new(mailer::ResendSender::new(c)) as std::sync::Arc<dyn mailer::Sender>
        });
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Every outbound client identifies itself, or is one of the two that
    /// deliberately does not.
    ///
    /// reqwest adds no User-Agent of its own, so a client built without
    /// `.user_agent(...)` is silently anonymous and nothing at runtime says so.
    /// That is how every poller fetch reached six third party services with no
    /// identification until 2026-10-04.
    ///
    /// Enumerated from the files that construct a client rather than from a list
    /// of the ones known to be correct, because a list cannot see the client
    /// somebody adds next. The floor below fails if the scan stops finding
    /// constructions at all, which is the way this check would otherwise rot
    /// into a pass.
    #[test]
    fn every_shipped_http_client_is_identified_or_named_as_an_exception() {
        // (file, source, how many non-test constructions it is allowed, why)
        let files: [(&str, &str); 3] = [
            ("main.rs", include_str!("main.rs")),
            ("mailer.rs", include_str!("mailer.rs")),
            ("webhook_guard.rs", include_str!("webhook_guard.rs")),
        ];

        // Resend goes through its own client and webhook delivery through
        // `webhook_guard`, which constrains the client for reasons unrelated to
        // identification. Both are named here so adding a third unidentified
        // client is a deliberate edit to this list.
        const EXCEPT: [&str; 2] = ["mailer.rs", "webhook_guard.rs"];

        let mut found = 0;
        for (name, src) in files {
            // Only the part of each file that ships. Test modules build throwaway
            // clients and are not what this protects.
            // Split on the attribute as it appears at the start of a line followed
            // by the module, not on the bare token. This crate root explains the
            // `forbid(unsafe_code)` exemption and writes `#[cfg(test)]` twice inside
            // a comment, so splitting on the token truncated main.rs before the
            // client it exists to check. The floor below is what caught that.
            let shipped = src
                .split(
                    "
#[cfg(test)]
mod ",
                )
                .next()
                .unwrap_or(src);
            let builds = shipped.matches("Client::builder()").count()
                + shipped.matches("Client::new()").count();
            found += builds;
            if builds == 0 || EXCEPT.contains(&name) {
                continue;
            }
            assert!(
                shipped.contains(".user_agent("),
                "{name} builds {builds} http client(s) and never calls .user_agent(..). \
                 Set crate::USER_AGENT on it, or add {name} to EXCEPT with a reason."
            );
        }
        assert!(
            found >= 3,
            "the scan found only {found} client constructions across {} files, \
             too few to conclude anything from",
            files.len()
        );
        assert!(
            !USER_AGENT.is_empty(),
            "USER_AGENT is empty, so setting it identifies nothing"
        );
    }

    /// The GitHub exchanges keep their own header.
    ///
    /// They share the client this crate builds, so the client default would now
    /// cover them. The explicit header stays because GitHub rejects a request
    /// without one, and a per-request header that depends on a client built
    /// elsewhere is the kind of coupling that breaks quietly.
    #[test]
    fn the_oauth_exchanges_set_the_header_themselves() {
        let src = include_str!("oauth.rs");
        let sites = src.matches(r#".header("User-Agent", USER_AGENT)"#).count();
        assert_eq!(
            sites, 2,
            "expected both GitHub exchanges to set User-Agent explicitly, found {sites}"
        );
    }
}
