use reqwest::Url;
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
pub type SendFuture<'a> =
    std::pin::Pin<Box<dyn std::future::Future<Output = SendOutcome> + Send + 'a>>;

/// What became of one message.
///
/// This was a `bool`, which could not tell a caller apart the two failures that
/// matter. On 2026-09-04 a verification mail to an address on the provider's
/// suppression list was accepted by the API, dropped, and reported to the user
/// as sent, because the send returned success and the endpoint answered 204.
/// `Suppressed` is that case. It is not a failure to retry: it says this
/// address receives nothing until somebody clears it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendOutcome {
    /// The provider took the message and will attempt delivery.
    Sent,
    /// The provider will not deliver to this address. Nothing was sent, and
    /// sending again changes nothing.
    Suppressed,
    /// The attempt failed. Trying again later may work.
    Failed,
}

/// Where mail goes.
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
///
/// The suppression check lives behind this trait rather than at the call sites.
/// A caller cannot forget what it never has to remember, and the thing being
/// protected is the claim that mail was sent, which only `SendOutcome::Sent`
/// makes. Every path to that claim runs through one implementation of this
/// trait, so a sixth mail path added later is covered by existing. The policy
/// belongs to the trait, the mechanism to `ResendSender`: only it knows the
/// answer comes from asking Resend.
pub trait Sender: Send + Sync {
    fn send_text<'a>(&'a self, to: &'a str, subject: &'a str, body: &'a str) -> SendFuture<'a>;
    fn send_html<'a>(&'a self, to: &'a str, subject: &'a str, html: &'a str) -> SendFuture<'a>;
}

/// What the provider says about an address before anything is sent to it.
enum Suppression {
    /// On the suppression list. Nothing sent to it will be delivered.
    Listed,
    /// Not on the list.
    Clear,
    /// The question could not be answered.
    Unknown,
}

/// How long to wait for the suppression answer before giving up on it.
///
/// One attempt, no retry. The answer only gates a send that would otherwise
/// happen anyway, so a second attempt buys little, and this sits inside a
/// request somebody is waiting on.
const SUPPRESSION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// Where the suppression lookup goes. Tests build a `ResendSender` with a
/// different base, so the classifier can be driven against a server that
/// answers on command.
///
/// It needs a seam of its own because the `Sender` seam is the wrong level for
/// this: `TestSender` decides suppression itself and never reaches the code
/// that reads the provider's answer. A mutation on 2026-09-04 proved the point,
/// turning a 200 into `Clear` with every test still passing, because nothing
/// exercised the mapping at all.
const SUPPRESSION_BASE: &str = "https://api.resend.com/suppressions/";

/// The real one.
pub struct ResendSender {
    config: MailerConfig,
    /// For the suppression lookup, which the Resend crate does not cover. Held
    /// rather than built per call so the lookup reuses a connection. `Resend`
    /// itself is still constructed per send, which is why a send pays a TLS
    /// handshake and this check does not.
    http: reqwest::Client,
    base: String,
}

impl ResendSender {
    pub fn new(config: MailerConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::new(),
            base: SUPPRESSION_BASE.to_string(),
        }
    }

    /// Asks whether the provider has stopped delivering to this address.
    ///
    /// `GET /suppressions/{email}` answers 200 with the record, or 404 when
    /// there is none, verified against the live API on 2026-09-04. The address
    /// goes in through `path_segments_mut` rather than `format!` because
    /// `validate_email` permits `?`, `#` and `/` in a local part: interpolated
    /// raw, `a?b@example.com` truncates the path and the 404 for `a` reads as a
    /// clear address.
    async fn suppression(&self, to: &str) -> Suppression {
        let mut url = match Url::parse(&self.base) {
            Ok(u) => u,
            Err(e) => {
                warn!("mailer: suppression url is not parseable: {e}");
                return Suppression::Unknown;
            }
        };
        match url.path_segments_mut() {
            Ok(mut segments) => {
                segments.pop_if_empty().push(to);
            }
            Err(()) => return Suppression::Unknown,
        }

        match self
            .http
            .get(url)
            .bearer_auth(&self.config.api_key)
            .timeout(SUPPRESSION_TIMEOUT)
            .send()
            .await
        {
            Ok(r) if r.status().as_u16() == 200 => Suppression::Listed,
            Ok(r) if r.status().as_u16() == 404 => Suppression::Clear,
            Ok(r) => {
                warn!(
                    "mailer: suppression lookup for {to} answered {}, treating as unknown",
                    r.status()
                );
                Suppression::Unknown
            }
            Err(e) => {
                warn!("mailer: suppression lookup for {to} failed: {e}");
                Suppression::Unknown
            }
        }
    }

    async fn deliver(&self, to: &str, subject: &str, opts: CreateEmailBaseOptions) -> SendOutcome {
        match self.suppression(to).await {
            Suppression::Listed => {
                warn!("mailer: {subject:?} not sent, {to} is on the provider suppression list");
                return SendOutcome::Suppressed;
            }
            // Fail open, deliberately. This check exists to stop a false
            // success on the account recovery path. Refusing to send because
            // the provider is unreachable would put a false failure on that
            // same path, which is worse: nothing tells the user it was our
            // outage and there is nothing they can do about it. Sending anyway
            // is also what this code did before the check existed, so an outage
            // degrades to the old behaviour rather than to a new one. Logged
            // rather than silent, because "we did not check" and "we checked
            // and it was clear" must not look the same afterwards.
            Suppression::Unknown => {
                warn!("mailer: sending {subject:?} to {to} without a suppression answer")
            }
            Suppression::Clear => {}
        }

        let client = Resend::new(&self.config.api_key);
        match client.emails.send(opts).await {
            Ok(_) => {
                info!("mailer: {subject:?} sent to {to}");
                SendOutcome::Sent
            }
            Err(e) => {
                warn!("mailer: send failed to {to}: {e}");
                SendOutcome::Failed
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
    /// Addresses this sender answers `Suppressed` for, standing in for the
    /// provider's list without reaching it.
    pub suppressed: Vec<String>,
    pub sent: std::sync::Mutex<Vec<(String, String, String)>>,
}

#[cfg(test)]
impl TestSender {
    pub fn accepting() -> Self {
        Self {
            result: true,
            suppressed: Vec::new(),
            sent: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn refusing() -> Self {
        Self {
            result: false,
            suppressed: Vec::new(),
            sent: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Accepts everything except the named address, which the provider has
    /// stopped delivering to.
    pub fn suppressing(address: &str) -> Self {
        Self {
            result: true,
            suppressed: vec![address.to_string()],
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
            // Recorded after the suppression answer, not before, so `count()`
            // means messages that left rather than messages attempted.
            if self.suppressed.iter().any(|s| s == to) {
                return SendOutcome::Suppressed;
            }
            if let Ok(mut s) = self.sent.lock() {
                s.push((to.to_string(), subject.to_string(), body.to_string()));
            }
            if self.result {
                SendOutcome::Sent
            } else {
                SendOutcome::Failed
            }
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
pub async fn send_verification_email(
    sender: &dyn Sender,
    to: &str,
    verify_url: &str,
) -> SendOutcome {
    let body = format!(
        "Welcome to Astraeusio!\n\nClick the link below to verify your email address:\n\n{verify_url}\n\nThis link expires in 1 hour.\n\nIf you did not create an account, you can safely ignore this email."
    );
    sender
        .send_text(to, "Verify your Astraeusio email address", &body)
        .await
}

pub async fn send_welcome_email(sender: &dyn Sender, to: &str, app_url: &str) -> SendOutcome {
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

    // Returned rather than discarded. The caller does not act on it, but a
    // suppressed welcome mail is the first visible sign that an address has
    // stopped accepting anything, and that is worth a log line at the call
    // site rather than nothing at all.
    sender.send_html(to, "Welcome to Astraeusio", &html).await
}

pub async fn send_password_reset_email(
    sender: &dyn Sender,
    to: &str,
    reset_url: &str,
) -> SendOutcome {
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

    sender
        .send_html(to, "Reset your Astraeusio password", &html)
        .await
}

#[cfg(test)]
mod outcome_tests {
    use super::*;

    /// Serves one status code on any path and returns its base URL.
    async fn server_answering(status: u16) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        let app = axum::Router::new().fallback(move || async move {
            axum::http::StatusCode::from_u16(status).expect("status")
        });
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        format!("http://{addr}/suppressions/")
    }

    /// Built field by field rather than through a constructor, so no test-only
    /// entry point sits in the production impl. One did, and because the source
    /// scans in this module cut the file at its first `#[cfg(test)]`, it hid
    /// everything below it from them.
    fn sender_with_base(base: &str) -> ResendSender {
        ResendSender {
            config: MailerConfig {
                api_key: "test-key".to_string(),
                from: "Astraeus <test@example.com>".to_string(),
            },
            http: reqwest::Client::new(),
            base: base.to_string(),
        }
    }

    /// What the provider's answer means. 200 is a listed address, 404 is a
    /// clear one, and anything else is no answer at all.
    ///
    /// Driven against a real socket rather than a stub, because the thing being
    /// checked is the mapping from an HTTP status, and a stub of the status
    /// would be the assertion restating itself. Reading 200 as `Clear` is the
    /// defect that would send mail into a black hole while reporting success,
    /// which is the whole finding this code exists for.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_provider_answer_is_classified_by_its_status() {
        for (status, want, label) in [
            (200u16, "Listed", "an address on the list"),
            (404, "Clear", "an address not on the list"),
            (500, "Unknown", "an answer that is neither"),
        ] {
            let base = server_answering(status).await;
            let sender = sender_with_base(&base);
            let got = match sender.suppression("someone@example.com").await {
                Suppression::Listed => "Listed",
                Suppression::Clear => "Clear",
                Suppression::Unknown => "Unknown",
            };
            assert_eq!(got, want, "{label}: HTTP {status} must read as {want}");
        }
    }

    /// A lookup that never answers is not a clear address.
    ///
    /// The timeout path reaches the same `Unknown` as a 500, but by a different
    /// route, and treating a hung provider as a clear address would send to
    /// every suppressed address during an outage.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_lookup_that_never_answers_reads_as_unknown() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        // Accepted and then left hanging, so the client waits on the timeout
        // rather than getting a connection refused.
        tokio::spawn(async move {
            loop {
                if let Ok((stream, _)) = listener.accept().await {
                    std::mem::forget(stream);
                }
            }
        });
        let sender = sender_with_base(&format!("http://{addr}/suppressions/"));
        assert!(
            matches!(
                sender.suppression("someone@example.com").await,
                Suppression::Unknown
            ),
            "a lookup that never answers must not read as a clear address"
        );
    }

    /// Strips comments and whitespace from a source file's production half.
    ///
    /// Comments matter here because the trait's own documentation names
    /// `SendOutcome::Sent` while explaining why the check lives behind it, and
    /// a raw count would read that sentence as a second place that can claim a
    /// message was sent.
    fn production_code(whole: &str) -> String {
        whole
            .split("#[cfg(test)]")
            .next()
            .unwrap_or(whole)
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<String>()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect()
    }

    /// Only one place in the codebase can say a message was sent.
    ///
    /// Enumerated from the thing being protected rather than from the places
    /// the check already appears: the asset is the claim that mail left, every
    /// such claim is a `SendOutcome::Sent`, and if it can only be built inside
    /// `deliver` then it cannot be built without the suppression lookup that
    /// runs first. A sixth mail path added later is covered by existing rather
    /// than by remembering.
    ///
    /// The floor is here because a scan that reads nothing finds nothing and
    /// passes, which is how a mutation harness in this repository reported
    /// three defects as unguarded while running zero tests.
    #[test]
    fn only_deliver_can_claim_a_message_was_sent() {
        let flat = production_code(include_str!("mailer.rs"));
        assert!(
            flat.len() > 2000,
            "read only {} characters of mailer.rs, so this scan proves nothing",
            flat.len()
        );
        assert_eq!(
            flat.matches("SendOutcome::Sent").count(),
            1,
            "SendOutcome::Sent must be constructed in exactly one place, inside deliver"
        );

        // The rule and its application are separate things. The classifier
        // being right says nothing about `deliver` consulting it, and a
        // classifier that is never called is the same as no check at all.
        let ask = flat
            .find("self.suppression(to).await")
            .expect("deliver must ask about the address");
        let send = flat
            .find("client.emails.send(opts).await")
            .expect("deliver must send");
        assert!(
            ask < send,
            "deliver sends before it asks whether the address accepts mail"
        );
    }

    /// Everywhere else may read that claim and may not make it.
    ///
    /// The two callers that care compare against `Sent`. If one of them ever
    /// returns it instead, a mail path exists that never asked the provider
    /// whether the address accepts mail.
    #[test]
    fn no_caller_outside_the_mailer_constructs_an_outcome() {
        for (file, whole) in [
            ("auth.rs", include_str!("auth.rs")),
            ("poller.rs", include_str!("poller.rs")),
        ] {
            let flat = production_code(whole);
            assert!(
                flat.len() > 2000,
                "read only {} characters of {file}, so this scan proves nothing",
                flat.len()
            );
            for (i, _) in flat.match_indices("SendOutcome::Sent") {
                // Walk left off the path qualifier first. The callers write
                // `mailer::SendOutcome::Sent`, so the two characters before the
                // match are the `::` of the module path, not the operator that
                // says whether this is a comparison or a construction. Reading
                // them directly is how this test failed on correct code.
                let mut start = i;
                while start > 0 {
                    let c = flat.as_bytes()[start - 1] as char;
                    if !(c.is_alphanumeric() || c == '_' || c == ':') {
                        break;
                    }
                    start -= 1;
                }
                let before = &flat[start.saturating_sub(2)..start];
                let after = &flat[i + "SendOutcome::Sent".len()..];
                // Two shapes read the value and neither builds one: an operand
                // of a comparison, and a match arm pattern. Anything else is
                // putting a `Sent` somewhere, which is the claim only `deliver`
                // is allowed to make.
                let is_read = before == "==" || before == "!=" || after.starts_with("=>");
                assert!(
                    is_read,
                    "{file} appears to construct SendOutcome::Sent rather than read it, \
                     preceded by {before:?}; only mailer.rs may construct one"
                );
            }
        }
    }

    /// An unanswered suppression lookup must not stop the send.
    ///
    /// This is the weaker of the tests in this file and deliberately so. The
    /// branch lives inside `deliver`, which only runs with a network, so there
    /// is no seam to drive it through: what is asserted is the shape of the
    /// source rather than the behaviour. It pins the one thing that would
    /// invert the trade, an early return under `Unknown`, which would turn a
    /// provider outage into an account lockout on the recovery path.
    #[test]
    fn an_unanswered_lookup_does_not_refuse_the_send() {
        let flat = production_code(include_str!("mailer.rs"));
        let unknown = flat
            .find("Suppression::Unknown=>")
            .expect("the Unknown arm must exist in deliver");
        let listed = flat
            .find("Suppression::Listed=>")
            .expect("the Listed arm must exist in deliver");
        assert!(listed < unknown, "expected Listed to be matched before Unknown");
        let arm = &flat[unknown..flat[unknown..].find("Suppression::Clear=>").map_or(flat.len(), |o| unknown + o)];
        assert!(
            !arm.contains("returnSendOutcome"),
            "the Unknown arm returns early, which fails closed: {arm}"
        );
    }
}
