use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::header::ContentType,
    transport::smtp::authentication::Credentials,
};
use tracing::{info, warn};

#[derive(Clone, Debug)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub from: String,
}

impl SmtpConfig {
    pub fn from_env() -> Option<Self> {
        let host = std::env::var("SMTP_HOST").ok()?;
        let port = std::env::var("SMTP_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(587);
        let user = std::env::var("SMTP_USER").ok()?;
        let password = std::env::var("SMTP_PASSWORD").ok()?;
        let from = std::env::var("SMTP_FROM").unwrap_or_else(|_| user.clone());
        // Skip loading if still placeholder values
        if host == "smtp.gmail.com" && user.contains("your@") {
            return None;
        }
        Some(Self { host, port, user, password, from })
    }
}

pub async fn send_alert_email(config: &SmtpConfig, to: &str, subject: &str, body: &str) {
    let from_box = match config.from.parse() {
        Ok(m) => m,
        Err(e) => {
            warn!("mailer: invalid from address '{}': {e}", config.from);
            return;
        }
    };
    let to_box = match to.parse() {
        Ok(m) => m,
        Err(e) => {
            warn!("mailer: invalid to address '{to}': {e}");
            return;
        }
    };

    let email = match Message::builder()
        .from(from_box)
        .to(to_box)
        .subject(subject)
        .header(ContentType::TEXT_PLAIN)
        .body(body.to_owned())
    {
        Ok(m) => m,
        Err(e) => {
            warn!("mailer: message build error: {e}");
            return;
        }
    };

    let creds = Credentials::new(config.user.clone(), config.password.clone());
    let transport = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host)
        .map(|b| b.port(config.port).credentials(creds).build());

    match transport {
        Ok(mailer) => match mailer.send(email).await {
            Ok(_) => info!("mailer: alert sent to {to}"),
            Err(e) => warn!("mailer: send failed to {to}: {e}"),
        },
        Err(e) => warn!("mailer: transport build error: {e}"),
    }
}
