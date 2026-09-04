//! Delivering one anomaly to one customer endpoint, and recording what happened
//! in terms the customer can act on.
//!
//! Two things here are constrained by AUD-004.
//!
//! The client is the guarded one from [`crate::webhook_guard`], not the shared
//! poller client, and the URL is validated again on the way out. Registration
//! already checked it, but an IP literal never reaches the guarded resolver, and
//! a row stored before the rules existed was never checked at all.
//!
//! The outcome is a token from a closed set rather than a `reqwest::Error`
//! string. The string carries the resolved address, the internal hostname and
//! whatever the TLS stack felt like saying, into a field the account owner reads
//! back through `GET /api/webhooks/{id}/deliveries`. The tokens below are chosen
//! to answer the only two questions an integrator actually has, did you reach me
//! and what did I say, without describing our network to somebody probing it.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use tracing::{info, warn};

use crate::db::WebhookRow;
use crate::webhook_guard::{self, GuardError, Rejection};

type HmacSha256 = Hmac<Sha256>;

/// The refused-before-sending case, which is the one an owner most needs told:
/// a webhook that never fires looks identical to one that is never triggered.
const BLOCKED_BY_POLICY: &str = "blocked_by_policy";
const DNS_FAILED: &str = "dns_failed";
const CONNECT_FAILED: &str = "connect_failed";
const TLS_FAILED: &str = "tls_failed";
const TIMEOUT: &str = "timeout";
const REDIRECT_REFUSED: &str = "redirect_refused";
const HTTP_ERROR: &str = "http_error";
const INTERNAL: &str = "internal";

pub struct DeliveryResult {
    pub status_code: Option<i32>,
    pub success: bool,
    pub error: Option<String>,
}

impl DeliveryResult {
    fn failed(reason: &str) -> Self {
        DeliveryResult {
            status_code: None,
            success: false,
            error: Some(reason.to_owned()),
        }
    }
}

/// Maps a `reqwest::Error` onto one token.
///
/// The guard's own refusal is found by walking the source chain and downcasting,
/// so it survives however many layers hyper wraps it in. The TLS case is the one
/// judgement call: reqwest reports a handshake failure as a connect error like
/// any other, so it is recognised by looking at the chain's text. A miss reports
/// `connect_failed`, which is a worse answer and not a leak, since none of the
/// text inspected here is ever returned or stored.
fn classify(e: &reqwest::Error) -> &'static str {
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(e);
    let mut chain = String::new();
    while let Some(err) = source {
        if let Some(guard) = err.downcast_ref::<GuardError>() {
            return match guard.0 {
                Rejection::PrivateAddress => BLOCKED_BY_POLICY,
                Rejection::Unresolvable => DNS_FAILED,
                _ => BLOCKED_BY_POLICY,
            };
        }
        chain.push_str(&err.to_string().to_ascii_lowercase());
        chain.push(' ');
        source = err.source();
    }

    if e.is_timeout() {
        return TIMEOUT;
    }
    if ["certificate", "tls", "ssl", "handshake"]
        .iter()
        .any(|marker| chain.contains(marker))
    {
        return TLS_FAILED;
    }
    if e.is_connect() {
        return CONNECT_FAILED;
    }
    INTERNAL
}

pub async fn send(
    client: &reqwest::Client,
    hook: &WebhookRow,
    event: &str,
    source_ref: &str,
    severity: &str,
    message: &str,
    timestamp: i64,
) -> DeliveryResult {
    // The rules can have changed, or tightened, since this row was stored.
    if let Err(rejection) = webhook_guard::validate_syntax(&hook.url) {
        warn!("webhook {} refused before sending: {}", hook.id, rejection);
        return DeliveryResult::failed(BLOCKED_BY_POLICY);
    }

    let payload = serde_json::json!({
        "event":     event,
        "severity":  severity,
        "message":   message,
        "timestamp": timestamp,
        "data": {
            "source_ref": source_ref,
        },
    });
    let body = payload.to_string();

    // HMAC takes a key of any length, so this cannot fail today. It is written
    // out rather than unwrapped because "cannot fail" is a property of the
    // current crate and not of this call site: if a future version of `hmac`
    // rejects a key, the delivery fails as `internal` and the log names the
    // webhook, instead of the process dying inside a spawned task.
    let mut mac = match HmacSha256::new_from_slice(hook.secret.as_bytes()) {
        Ok(m) => m,
        Err(e) => {
            warn!("webhook {} signing failed: {e}", hook.id);
            return DeliveryResult::failed(INTERNAL);
        }
    };
    mac.update(body.as_bytes());
    let sig = hex::encode(mac.finalize().into_bytes());

    match client
        .post(&hook.url)
        .header("Content-Type", "application/json")
        .header("X-Astraeus-Signature", format!("sha256={sig}"))
        .header("X-Astraeus-Event", event)
        .timeout(std::time::Duration::from_secs(5))
        .body(body)
        .send()
        .await
    {
        Ok(r) => {
            let code = r.status().as_u16() as i32;
            // The customer's own URL, which often carries their token in the
            // query string. Their secret deserves the same treatment as ours.
            info!(
                "webhook {}: {} -> {}",
                hook.id,
                crate::redact::secrets(&hook.url),
                r.status()
            );
            // A 3xx arrives as a response rather than an error because the
            // delivery client does not follow redirects. Naming it separately
            // tells the owner to register the final URL.
            let error = match code {
                200..=299 => None,
                300..=399 => Some(REDIRECT_REFUSED.to_owned()),
                _ => Some(HTTP_ERROR.to_owned()),
            };
            DeliveryResult {
                status_code: Some(code),
                success: (200..300).contains(&code),
                error,
            }
        }
        Err(e) => {
            let reason = classify(&e);
            warn!(
                "webhook {} delivery failed ({}): {}",
                hook.id,
                reason,
                crate::redact::secrets(&e.to_string())
            );
            DeliveryResult::failed(reason)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hook(url: &str) -> WebhookRow {
        WebhookRow {
            id: "hook1".to_owned(),
            url: url.to_owned(),
            secret: "s3cret".to_owned(),
            events: vec!["kp_storm".to_owned()],
            created_at: 0,
        }
    }

    /// A row that predates the rules, or one written when they were looser, is
    /// refused on its way out rather than delivered. No request is made, so this
    /// holds even for an address the guarded resolver never sees.
    #[tokio::test]
    async fn a_stored_internal_target_is_refused_at_delivery() {
        let client = webhook_guard::client(std::time::Duration::from_secs(5))
            .expect("the delivery client builds");

        for url in [
            "http://169.254.169.254/latest/meta-data/",
            "http://127.0.0.1:3000/api/user/me",
            "https://10.0.0.1/",
            "https://[::1]/",
        ] {
            let r = send(
                &client,
                &hook(url),
                "kp_storm",
                "ref",
                "warning",
                "message",
                0,
            )
            .await;
            assert!(!r.success, "{url} must not be delivered");
            assert_eq!(r.status_code, None, "{url} must not reach a server");
            assert_eq!(r.error.as_deref(), Some(BLOCKED_BY_POLICY), "{url}");
        }
    }

    /// A name is judged by the guarded resolver rather than by how it is
    /// spelled, and the refusal has to survive being wrapped by hyper and
    /// reqwest on its way back. This is the test that fails if the resolver is
    /// ever dropped from the delivery client, which is the whole rebinding
    /// defence: `localhost` is syntactically an ordinary name and only its
    /// answer is disqualifying.
    #[tokio::test]
    async fn a_name_resolving_into_a_reserved_range_is_blocked_at_connect() {
        let client = webhook_guard::client(std::time::Duration::from_secs(5))
            .expect("the delivery client builds");

        let r = send(
            &client,
            &hook("https://localhost:3000/hook"),
            "kp_storm",
            "ref",
            "warning",
            "message",
            0,
        )
        .await;

        assert!(!r.success);
        assert_eq!(r.status_code, None);
        assert_eq!(r.error.as_deref(), Some(BLOCKED_BY_POLICY));
    }

    /// The delivery log never carries a `reqwest` error string, whichever way a
    /// delivery fails.
    ///
    /// This one asserts the closed set rather than a particular token on
    /// purpose. Which failure an unresolvable name produces is a property of the
    /// machine: where the stub resolver answers quickly it is `dns_failed`, and
    /// on a host whose resolver sits on a dead `.invalid` lookup the request
    /// timeout fires first and it is `timeout`. Both are correct and neither is
    /// a library string, which is the property under test.
    #[tokio::test]
    async fn a_failed_delivery_reports_a_token_and_not_a_library_string() {
        let client = webhook_guard::client(std::time::Duration::from_secs(5))
            .expect("the delivery client builds");

        let r = send(
            &client,
            &hook("https://no-such-host.invalid/hook"),
            "kp_storm",
            "ref",
            "warning",
            "message",
            0,
        )
        .await;

        assert!(!r.success);
        let token = r.error.expect("a failed delivery records a reason");
        assert!(
            [
                BLOCKED_BY_POLICY,
                DNS_FAILED,
                CONNECT_FAILED,
                TLS_FAILED,
                TIMEOUT,
                REDIRECT_REFUSED,
                HTTP_ERROR,
                INTERNAL,
            ]
            .contains(&token.as_str()),
            "unexpected delivery error token: {token}"
        );
    }
}
