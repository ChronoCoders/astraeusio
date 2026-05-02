use hmac::{Hmac, Mac};
use sha2::Sha256;
use tracing::{info, warn};

use crate::db::WebhookRow;

type HmacSha256 = Hmac<Sha256>;

pub async fn send(
    client: &reqwest::Client,
    hook: &WebhookRow,
    event: &str,
    source_ref: &str,
    severity: &str,
    message: &str,
    timestamp: i64,
) {
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

    let mut mac = HmacSha256::new_from_slice(hook.secret.as_bytes())
        .expect("HMAC accepts any key size");
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
        Ok(r) => info!("webhook {}: {} -> {}", hook.id, hook.url, r.status()),
        Err(e) => warn!("webhook {} delivery failed: {e}", hook.id),
    }
}
