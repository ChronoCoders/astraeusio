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

pub async fn send_verification_email(config: &MailerConfig, to: &str, verify_url: &str) {
    let body = format!(
        "Welcome to Astraeusio!\n\nClick the link below to verify your email address:\n\n{verify_url}\n\nThis link expires in 24 hours.\n\nIf you did not create an account, you can safely ignore this email."
    );
    send_alert_email(config, to, "Verify your Astraeusio email address", &body).await;
}

pub async fn send_welcome_email(config: &MailerConfig, to: &str, app_url: &str) {
    let dashboard_url = app_url.to_string();
    let api_keys_url  = format!("{app_url}/api-keys");
    let docs_url      = format!("{app_url}/docs");

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>Welcome to Astraeusio</title>
</head>
<body style="margin:0;padding:0;background:#09090b;font-family:system-ui,-apple-system,sans-serif;color:#e4e4e7;">
  <table width="100%" cellpadding="0" cellspacing="0" style="background:#09090b;padding:48px 16px;">
    <tr>
      <td align="center">
        <table width="100%" cellpadding="0" cellspacing="0" style="max-width:520px;">

          <!-- Wordmark -->
          <tr>
            <td style="padding-bottom:40px;">
              <span style="font-size:13px;font-family:monospace;letter-spacing:0.25em;color:#e4e4e7;font-weight:300;">ASTRAEUSIO</span>
            </td>
          </tr>

          <!-- Heading -->
          <tr>
            <td style="padding-bottom:16px;border-top:1px solid #27272a;padding-top:32px;">
              <h1 style="margin:0;font-size:22px;font-weight:300;color:#f4f4f5;letter-spacing:-0.02em;line-height:1.3;">
                Your email is verified.
              </h1>
            </td>
          </tr>

          <!-- Body -->
          <tr>
            <td style="padding-bottom:32px;">
              <p style="margin:0 0 14px;font-size:14px;line-height:1.65;color:#a1a1aa;">
                Welcome aboard. Your account is ready — you now have access to real-time space weather data,
                ML-powered Kp forecasts, anomaly detection, and the full API.
              </p>
              <p style="margin:0;font-size:14px;line-height:1.65;color:#a1a1aa;">
                Here are a few things to explore first:
              </p>
            </td>
          </tr>

          <!-- Links -->
          <tr>
            <td style="padding-bottom:32px;">
              <table width="100%" cellpadding="0" cellspacing="0">
                <tr>
                  <td style="padding:14px 0;border-top:1px solid #27272a;">
                    <a href="{dashboard_url}" style="text-decoration:none;">
                      <span style="font-size:13px;font-weight:500;color:#f4f4f5;">Dashboard</span>
                      <span style="font-size:12px;color:#71717a;margin-left:8px;">→</span>
                    </a>
                    <p style="margin:4px 0 0;font-size:12px;color:#71717a;line-height:1.5;">
                      Live Kp index, solar wind, ML forecast, ISS position, and more.
                    </p>
                  </td>
                </tr>
                <tr>
                  <td style="padding:14px 0;border-top:1px solid #27272a;">
                    <a href="{api_keys_url}" style="text-decoration:none;">
                      <span style="font-size:13px;font-weight:500;color:#f4f4f5;">API Keys</span>
                      <span style="font-size:12px;color:#71717a;margin-left:8px;">→</span>
                    </a>
                    <p style="margin:4px 0 0;font-size:12px;color:#71717a;line-height:1.5;">
                      Generate keys and start querying the API programmatically.
                    </p>
                  </td>
                </tr>
                <tr>
                  <td style="padding:14px 0;border-top:1px solid #27272a;border-bottom:1px solid #27272a;">
                    <a href="{docs_url}" style="text-decoration:none;">
                      <span style="font-size:13px;font-weight:500;color:#f4f4f5;">Documentation</span>
                      <span style="font-size:12px;color:#71717a;margin-left:8px;">→</span>
                    </a>
                    <p style="margin:4px 0 0;font-size:12px;color:#71717a;line-height:1.5;">
                      Endpoint reference, authentication, and integration guides.
                    </p>
                  </td>
                </tr>
              </table>
            </td>
          </tr>

          <!-- CTA -->
          <tr>
            <td style="padding-bottom:40px;">
              <a href="{dashboard_url}"
                 style="display:inline-block;background:#f4f4f5;color:#09090b;font-size:13px;font-family:monospace;
                        letter-spacing:0.05em;padding:10px 24px;border-radius:6px;text-decoration:none;font-weight:500;">
                Open Dashboard
              </a>
            </td>
          </tr>

          <!-- Footer -->
          <tr>
            <td style="border-top:1px solid #27272a;padding-top:24px;">
              <p style="margin:0;font-size:11px;color:#52525b;line-height:1.6;font-family:monospace;">
                Questions? Reply to this email or contact us at
                <a href="mailto:contact@chronocoder.dev" style="color:#71717a;text-decoration:none;">contact@chronocoder.dev</a>
              </p>
              <p style="margin:8px 0 0;font-size:11px;color:#3f3f46;font-family:monospace;">
                © 2026 Astraeusio · Built on NOAA and NASA open data
              </p>
            </td>
          </tr>

        </table>
      </td>
    </tr>
  </table>
</body>
</html>"#
    );

    let client = Resend::new(&config.api_key);
    let email = CreateEmailBaseOptions::new(&config.from, [to], "Welcome to Astraeusio")
        .with_html(&html);

    match client.emails.send(email).await {
        Ok(_)  => info!("mailer: welcome email sent to {to}"),
        Err(e) => warn!("mailer: welcome send failed to {to}: {e}"),
    }
}

pub async fn send_password_reset_email(config: &MailerConfig, to: &str, reset_url: &str) {
    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>Reset your Astraeusio password</title>
</head>
<body style="margin:0;padding:0;background:#09090b;font-family:system-ui,-apple-system,sans-serif;color:#e4e4e7;">
  <table width="100%" cellpadding="0" cellspacing="0" style="background:#09090b;padding:48px 16px;">
    <tr>
      <td align="center">
        <table width="100%" cellpadding="0" cellspacing="0" style="max-width:520px;">

          <tr>
            <td style="padding-bottom:40px;">
              <span style="font-size:13px;font-family:monospace;letter-spacing:0.25em;color:#e4e4e7;font-weight:300;">ASTRAEUSIO</span>
            </td>
          </tr>

          <tr>
            <td style="padding-bottom:16px;border-top:1px solid #27272a;padding-top:32px;">
              <h1 style="margin:0;font-size:22px;font-weight:300;color:#f4f4f5;letter-spacing:-0.02em;line-height:1.3;">
                Reset your password
              </h1>
            </td>
          </tr>

          <tr>
            <td style="padding-bottom:32px;">
              <p style="margin:0;font-size:14px;line-height:1.65;color:#a1a1aa;">
                We received a request to reset the password for your account. Click the button below to set a new password.
                This link expires in <strong style="color:#e4e4e7;">1 hour</strong>.
              </p>
            </td>
          </tr>

          <tr>
            <td style="padding-bottom:32px;">
              <a href="{reset_url}"
                 style="display:inline-block;background:#f4f4f5;color:#09090b;font-size:13px;font-family:monospace;
                        letter-spacing:0.05em;padding:10px 24px;border-radius:6px;text-decoration:none;font-weight:500;">
                Reset Password
              </a>
            </td>
          </tr>

          <tr>
            <td style="border-top:1px solid #27272a;padding-top:24px;">
              <p style="margin:0;font-size:11px;color:#52525b;line-height:1.6;font-family:monospace;">
                If you did not request a password reset, you can safely ignore this email. Your password will not change.
              </p>
              <p style="margin:8px 0 0;font-size:11px;color:#3f3f46;font-family:monospace;">
                © 2026 Astraeusio · Built on NOAA and NASA open data
              </p>
            </td>
          </tr>

        </table>
      </td>
    </tr>
  </table>
</body>
</html>"#
    );

    let client = Resend::new(&config.api_key);
    let email = CreateEmailBaseOptions::new(&config.from, [to], "Reset your Astraeusio password")
        .with_html(&html);

    match client.emails.send(email).await {
        Ok(_)  => info!("mailer: password reset email sent to {to}"),
        Err(e) => warn!("mailer: password reset send failed to {to}: {e}"),
    }
}

pub async fn send_alert_email(config: &MailerConfig, to: &str, subject: &str, body: &str) {
    let client = Resend::new(&config.api_key);
    let email = CreateEmailBaseOptions::new(&config.from, [to], subject).with_text(body);

    match client.emails.send(email).await {
        Ok(_)  => info!("mailer: alert sent to {to}"),
        Err(e) => warn!("mailer: send failed to {to}: {e}"),
    }
}
