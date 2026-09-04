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

/// A future returned by a `Sender`, boxed so the trait is dyn compatible.
///
/// Hand written rather than pulling in `async-trait` for one trait with two
/// methods.
pub type SendFuture<'a> = std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send + 'a>>;

/// Where mail goes. `true` means the provider accepted it.
///
/// This exists so two behaviours can be tested that could not be before, both
/// of which turn on a send having failed:
///
///   - `resend_verification` must report the failure rather than answer 204,
///     because that mail is the only way back for an account the verification
///     gate has locked out
///   - the email alert cooldown must be recorded only after a send succeeds,
///     since marking it first buys an hour of silence for an alert nobody got
///
/// Both were unreachable in a test while `Resend::new` was constructed inside
/// the function that sends: the only way to reach the failure branch was a real
/// network call with a bad key. Mutation testing on 2026-09-02 confirmed the
/// first was unguarded, with the failure branch removed and all 189 tests still
/// passing.
///
/// Scoped to the mailer's own sends and no wider. It covers text and HTML
/// because `AppState` holds one mailer: seaming only the text path would mean
/// carrying a `MailerConfig` beside the `Sender` for the two HTML callers, and
/// two sources for one thing is the shape that goes wrong later.
pub trait Sender: Send + Sync {
    fn send_text<'a>(&'a self, to: &'a str, subject: &'a str, body: &'a str) -> SendFuture<'a>;
    fn send_html<'a>(&'a self, to: &'a str, subject: &'a str, html: &'a str) -> SendFuture<'a>;
}

/// The real one.
pub struct ResendSender {
    config: MailerConfig,
}

impl ResendSender {
    pub fn new(config: MailerConfig) -> Self {
        Self { config }
    }

    async fn deliver(&self, to: &str, subject: &str, opts: CreateEmailBaseOptions) -> bool {
        let client = Resend::new(&self.config.api_key);
        match client.emails.send(opts).await {
            Ok(_) => {
                info!("mailer: {subject:?} sent to {to}");
                true
            }
            Err(e) => {
                warn!("mailer: send failed to {to}: {e}");
                false
            }
        }
    }
}

impl Sender for ResendSender {
    fn send_text<'a>(&'a self, to: &'a str, subject: &'a str, body: &'a str) -> SendFuture<'a> {
        Box::pin(async move {
            let opts =
                CreateEmailBaseOptions::new(&self.config.from, [to], subject).with_text(body);
            self.deliver(to, subject, opts).await
        })
    }

    fn send_html<'a>(&'a self, to: &'a str, subject: &'a str, html: &'a str) -> SendFuture<'a> {
        Box::pin(async move {
            let opts =
                CreateEmailBaseOptions::new(&self.config.from, [to], subject).with_html(html);
            self.deliver(to, subject, opts).await
        })
    }
}

/// A sender that answers however a test needs and reaches no network.
///
/// `#[cfg(test)]` so it cannot reach a release build at all, rather than being
/// a variant that merely should not be constructed there.
#[cfg(test)]
#[derive(Default)]
pub struct TestSender {
    pub result: bool,
    pub sent: std::sync::Mutex<Vec<(String, String, String)>>,
}

#[cfg(test)]
impl TestSender {
    pub fn accepting() -> Self {
        Self {
            result: true,
            sent: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn refusing() -> Self {
        Self {
            result: false,
            sent: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn count(&self) -> usize {
        self.sent.lock().map(|s| s.len()).unwrap_or(0)
    }

    pub fn recipients(&self) -> Vec<String> {
        self.sent
            .lock()
            .map(|s| s.iter().map(|(to, _, _)| to.clone()).collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
impl Sender for TestSender {
    fn send_text<'a>(&'a self, to: &'a str, subject: &'a str, body: &'a str) -> SendFuture<'a> {
        Box::pin(async move {
            if let Ok(mut s) = self.sent.lock() {
                s.push((to.to_string(), subject.to_string(), body.to_string()));
            }
            self.result
        })
    }

    fn send_html<'a>(&'a self, to: &'a str, subject: &'a str, html: &'a str) -> SendFuture<'a> {
        self.send_text(to, subject, html)
    }
}

/// Returns whether the provider accepted the message.
///
/// The result used to be discarded. That is tolerable for a welcome mail and
/// wrong for this one: once verification gates anything, this mail is the only
/// route back for an account that cannot get in, and a caller that cannot tell
/// a send from a silent failure cannot tell the user either.
pub async fn send_verification_email(sender: &dyn Sender, to: &str, verify_url: &str) -> bool {
    let body = format!(
        "Welcome to Astraeusio!\n\nClick the link below to verify your email address:\n\n{verify_url}\n\nThis link expires in 1 hour.\n\nIf you did not create an account, you can safely ignore this email."
    );
    sender
        .send_text(to, "Verify your Astraeusio email address", &body)
        .await
}

pub async fn send_welcome_email(sender: &dyn Sender, to: &str, app_url: &str) {
    let dashboard_url = app_url.to_string();
    let api_keys_url = format!("{app_url}/api-keys");
    let docs_url = format!("{app_url}/docs");

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
                Welcome aboard. Your account is ready - you now have access to real-time space weather data,
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
                <a href="mailto:hello@astraeusio.com" style="color:#71717a;text-decoration:none;">hello@astraeusio.com</a>
              </p>
              <p style="margin:8px 0 0;font-size:11px;color:#3f3f46;font-family:monospace;">
                © 2026 Astraeusio · All rights reserved.
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

    // Result deliberately discarded: a welcome mail that does not arrive costs
    // nothing recoverable, unlike the verification mail below it.
    let _ = sender.send_html(to, "Welcome to Astraeusio", &html).await;
}

pub async fn send_password_reset_email(sender: &dyn Sender, to: &str, reset_url: &str) {
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
                © 2026 Astraeusio · All rights reserved.
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

    let _ = sender
        .send_html(to, "Reset your Astraeusio password", &html)
        .await;
}
