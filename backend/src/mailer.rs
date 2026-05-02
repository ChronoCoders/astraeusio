use resend_rs::{Resend, types::CreateEmailBaseOptions};
use tracing::{info, warn};

#[derive(Clone, Debug)]
pub struct MailerConfig {
    pub api_key: String,
    pub from: String,
}

impl MailerConfig {
    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("RESEND_API_KEY").ok()?;
        if api_key.is_empty() || api_key.starts_with("re_YOUR") {
            return None;
        }
        let from = std::env::var("RESEND_FROM")
            .unwrap_or_else(|_| "Astraeus <onboarding@resend.dev>".to_string());
        Some(Self { api_key, from })
    }
}

pub async fn send_alert_email(config: &MailerConfig, to: &str, subject: &str, body: &str) {
    let client = Resend::new(&config.api_key);
    let email = CreateEmailBaseOptions::new(&config.from, [to], subject)
        .with_text(body);

    match client.emails.send(email).await {
        Ok(_) => info!("mailer: alert sent to {to}"),
        Err(e) => warn!("mailer: send failed to {to}: {e}"),
    }
}
