//! Social login (GitHub, Google) via the OAuth2 authorization-code flow.
//!
//! Flow:
//!   1. `GET /auth/oauth/{provider}/start` → 302 to the provider's consent page,
//!      carrying a signed, short-lived `state` JWT (stateless CSRF protection).
//!   2. Provider redirects back to `GET /auth/oauth/{provider}/callback`.
//!   3. We verify `state`, exchange the code for an access token, fetch the
//!      provider's *verified* email, then auto-link by email: an existing account
//!      is signed in; a new email creates a password-less account (email pre-verified).
//!   4. We redirect to the frontend at `{app_url}/oauth/callback#…` with either a
//!      full session token, or - if the account has TOTP enabled - a 2FA partial
//!      token (2FA is enforced even for social logins).
//!
//! A provider whose client id/secret are unset is disabled (its buttons are hidden
//! by the frontend, and `start` redirects back with an error).

use axum::{
    extract::{Path, Query, State},
    response::{IntoResponse, Redirect, Response},
    Json,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::{auth, routes::AppState};

const USER_AGENT: &str = "astraeusio";
/// Audience of the OAuth state token. It is minted for an anonymous caller by
/// the start endpoint, so it must never validate as a session.
const AUD_OAUTH_STATE: &str = "astraeus:oauth_state";

const STATE_TTL_SECS: i64 = 600;

// ── Config ──────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct ProviderCreds {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Clone, Default)]
pub struct OAuthConfig {
    pub github: Option<ProviderCreds>,
    pub google: Option<ProviderCreds>,
    /// Public base URL the provider redirects back to (the backend, reachable as
    /// `{base}/auth/oauth/{provider}/callback`). Defaults to `app_url`.
    pub redirect_base: String,
}

impl OAuthConfig {
    pub fn from_env(app_url: &str) -> Self {
        let creds = |id_key: &str, secret_key: &str| match (
            std::env::var(id_key),
            std::env::var(secret_key),
        ) {
            (Ok(id), Ok(secret)) if !id.is_empty() && !secret.is_empty() => {
                Some(ProviderCreds {
                    client_id: id,
                    client_secret: secret,
                })
            }
            _ => None,
        };
        OAuthConfig {
            github: creds("GITHUB_CLIENT_ID", "GITHUB_CLIENT_SECRET"),
            google: creds("GOOGLE_CLIENT_ID", "GOOGLE_CLIENT_SECRET"),
            redirect_base: std::env::var("OAUTH_REDIRECT_BASE")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| app_url.to_string()),
        }
    }

    /// Whether cookies for this deployment carry `Secure`.
    ///
    /// Read from the URL the provider is told to redirect back to, because that
    /// is the origin the cookie has to survive on. A deployment served over
    /// plain http gets a cookie without `Secure`, which is the only way sign in
    /// works there at all; astraeusio.com is https and gets it.
    pub fn cookies_are_secure(&self) -> bool {
        self.redirect_base.starts_with("https://")
    }

    fn creds(&self, provider: &str) -> Option<&ProviderCreds> {
        match provider {
            "github" => self.github.as_ref(),
            "google" => self.google.as_ref(),
            _ => None,
        }
    }

    pub fn enabled(&self) -> Vec<&'static str> {
        let mut v = Vec::new();
        if self.github.is_some() {
            v.push("github");
        }
        if self.google.is_some() {
            v.push("google");
        }
        v
    }
}

// ── State token (CSRF) ────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
struct StateClaims {
    provider: String,
    nonce: String,
    exp: u64,
    aud: String,
}

fn random_hex(n_bytes: usize) -> String {
    use rand::RngCore;
    let mut buf = vec![0u8; n_bytes];
    rand::thread_rng().fill_bytes(&mut buf);
    hex::encode(buf)
}

/// Name of the cookie holding the nonce that binds a flow to one browser.
const STATE_COOKIE: &str = "astraeus_oauth_nonce";

/// Returns the state token and the nonce inside it.
///
/// The nonce used to be generated here and thrown away, which made the state
/// token prove only that this server issued *some* state for this provider in
/// the last ten minutes. Any anonymous caller can cause that by requesting
/// `start`. The caller now puts the nonce in a cookie, so the token proves the
/// callback belongs to the browser that began the flow.
pub(crate) fn sign_state(
    provider: &str,
    secret: &str,
) -> Result<(String, String), jsonwebtoken::errors::Error> {
    let exp = (chrono::Utc::now().timestamp() + STATE_TTL_SECS) as u64;
    let nonce = random_hex(16);
    let token = encode(
        &Header::default(),
        &StateClaims {
            provider: provider.to_string(),
            nonce: nonce.clone(),
            exp,
            aud: AUD_OAUTH_STATE.to_string(),
        },
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;
    Ok((token, nonce))
}

/// The nonce carried by a valid state token for this provider, or `None`.
///
/// Returns the nonce rather than a boolean because the signature is only half
/// the check: the caller has to compare it against the cookie.
fn verify_state(token: &str, provider: &str, secret: &str) -> Option<String> {
    // The state token is handed to an anonymous caller by the start endpoint,
    // so it must not validate anywhere else. It carries no `sub`, so its
    // required claims differ from every other token this service mints.
    let mut validation = Validation::default();
    validation.set_audience(&[AUD_OAUTH_STATE]);
    validation.set_required_spec_claims(&["exp", "aud"]);
    decode::<StateClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .ok()
    .filter(|d| d.claims.provider == provider)
    .map(|d| d.claims.nonce)
}

/// `Set-Cookie` for the flow's nonce.
///
/// `SameSite=Lax` and not `Strict`: the callback arrives as a top level
/// navigation from the provider's domain, which `Strict` would strip the cookie
/// from, breaking every sign in. `Lax` sends it on exactly that navigation and
/// withholds it from cross site subrequests, which is the property wanted.
///
/// `Secure` follows the deployment's own scheme rather than being hardcoded.
/// Always setting it would break a plain http origin silently, in the direction
/// where sign in stops working for a reason nothing logs; never setting it
/// would ship a production cookie over http. The scheme is a fact about the
/// deployment, not a switch for tests.
fn state_cookie(nonce: &str, secure: bool) -> String {
    let mut c = format!(
        "{STATE_COOKIE}={nonce}; HttpOnly; SameSite=Lax; Path=/auth/oauth; Max-Age={STATE_TTL_SECS}"
    );
    if secure {
        c.push_str("; Secure");
    }
    c
}

/// The same cookie with an immediate expiry, so one flow's nonce cannot be
/// replayed into a second callback.
fn clear_state_cookie(secure: bool) -> String {
    let mut c = format!("{STATE_COOKIE}=; HttpOnly; SameSite=Lax; Path=/auth/oauth; Max-Age=0");
    if secure {
        c.push_str("; Secure");
    }
    c
}

/// The flow nonce from a request's `Cookie` header.
///
/// Hand written rather than pulling in a cookie crate for one value. Cookies
/// are `name=value` pairs separated by `; `, and a name that is a prefix of
/// another must not match, which is why this splits on `=` instead of using
/// `starts_with`.
fn nonce_from_cookies(headers: &axum::http::HeaderMap) -> Option<String> {
    let raw = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    raw.split(';')
        .filter_map(|kv| kv.split_once('='))
        .find(|(k, _)| k.trim() == STATE_COOKIE)
        .map(|(_, v)| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Constant time equality for two nonces.
///
/// Not because a timing attack on a per flow random value is realistic, but
/// because the alternative costs nothing and the next person reading this does
/// not have to work out whether it was realistic.
fn nonces_match(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

// ── Redirect helpers ──────────────────────────────────────────────────────────

fn frontend_redirect(app_url: &str, fragment: &str) -> Response {
    Redirect::to(&format!("{app_url}/oauth/callback#{fragment}")).into_response()
}

fn error_redirect(app_url: &str, code: &str) -> Response {
    frontend_redirect(app_url, &format!("error={code}"))
}

// ── Public: which providers are configured ─────────────────────────────────────

pub async fn list_providers(State(s): State<AppState>) -> Response {
    Json(serde_json::json!({ "providers": s.oauth.enabled() })).into_response()
}

// ── Start: redirect to provider consent ─────────────────────────────────────────

pub async fn start(Path(provider): Path<String>, State(s): State<AppState>) -> Response {
    let Some(creds) = s.oauth.creds(&provider) else {
        return error_redirect(&s.app_url, "provider_unavailable");
    };

    let (state, nonce) = match sign_state(&provider, &s.jwt_secret) {
        Ok(t) => t,
        Err(e) => {
            warn!("oauth state sign error: {e}");
            return error_redirect(&s.app_url, "oauth_failed");
        }
    };

    let redirect_uri = format!("{}/auth/oauth/{}/callback", s.oauth.redirect_base, provider);

    let url = match provider.as_str() {
        "github" => reqwest::Url::parse_with_params(
            "https://github.com/login/oauth/authorize",
            &[
                ("client_id", creds.client_id.as_str()),
                ("redirect_uri", redirect_uri.as_str()),
                ("scope", "read:user user:email"),
                ("state", state.as_str()),
                ("allow_signup", "true"),
            ],
        ),
        "google" => reqwest::Url::parse_with_params(
            "https://accounts.google.com/o/oauth2/v2/auth",
            &[
                ("client_id", creds.client_id.as_str()),
                ("redirect_uri", redirect_uri.as_str()),
                ("response_type", "code"),
                ("scope", "openid email profile"),
                ("state", state.as_str()),
                ("access_type", "online"),
                ("prompt", "select_account"),
            ],
        ),
        _ => return error_redirect(&s.app_url, "provider_unavailable"),
    };

    match url {
        Ok(u) => {
            // The nonce goes to the browser here and comes back on the callback.
            // The state token alone proved only that this server had issued
            // some state, which any anonymous caller can arrange.
            let mut resp = Redirect::to(u.as_str()).into_response();
            match axum::http::HeaderValue::from_str(&state_cookie(
                &nonce,
                s.oauth.cookies_are_secure(),
            )) {
                Ok(v) => {
                    resp.headers_mut().insert(axum::http::header::SET_COOKIE, v);
                    resp
                }
                // A flow whose nonce cannot be set is a flow with no CSRF
                // protection, so it does not start.
                Err(e) => {
                    warn!("oauth state cookie error: {e}");
                    error_redirect(&s.app_url, "oauth_failed")
                }
            }
        }
        Err(e) => {
            warn!("oauth authorize url error: {e}");
            error_redirect(&s.app_url, "oauth_failed")
        }
    }
}

// ── Callback ────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CallbackParams {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

/// Completes a flow, and spends its nonce whatever the outcome.
///
/// The clearing happens here and not inside `callback_inner`, which has a dozen
/// ways out. Attaching it to each of them is a rule that has to be remembered
/// twelve times and again on the thirteenth; attaching it to the one value they
/// all become is a rule that cannot be missed.
pub async fn callback(
    Path(provider): Path<String>,
    Query(params): Query<CallbackParams>,
    State(s): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Response {
    let secure = s.oauth.cookies_are_secure();
    let mut resp = callback_inner(provider, params, s, headers).await;
    if let Ok(v) = axum::http::HeaderValue::from_str(&clear_state_cookie(secure)) {
        resp.headers_mut().insert(axum::http::header::SET_COOKIE, v);
    }
    resp
}

async fn callback_inner(
    provider: String,
    params: CallbackParams,
    s: AppState,
    headers: axum::http::HeaderMap,
) -> Response {
    let app_url = s.app_url.clone();

    // User denied consent, or provider returned an error.
    if params.error.is_some() {
        return error_redirect(&app_url, "access_denied");
    }
    let (Some(code), Some(state)) = (params.code, params.state) else {
        return error_redirect(&app_url, "oauth_failed");
    };
    // Two halves, and both are needed. The signature says this server issued
    // the state; the cookie says this browser is the one it was issued to.
    // Without the second, an attacker completes a flow with their own account
    // and induces a victim's browser to load the callback, signing the victim
    // in as the attacker.
    let Some(signed_nonce) = verify_state(&state, &provider, &s.jwt_secret) else {
        return error_redirect(&app_url, "bad_state");
    };
    let Some(cookie_nonce) = nonce_from_cookies(&headers) else {
        warn!(provider = %provider, "oauth callback with no state cookie");
        return error_redirect(&app_url, "bad_state");
    };
    if !nonces_match(&signed_nonce, &cookie_nonce) {
        warn!(provider = %provider, "oauth callback state cookie does not match the state token");
        return error_redirect(&app_url, "bad_state");
    }
    let Some(creds) = s.oauth.creds(&provider) else {
        return error_redirect(&app_url, "provider_unavailable");
    };

    let redirect_uri = format!("{}/auth/oauth/{}/callback", s.oauth.redirect_base, provider);

    let email = match provider.as_str() {
        "github" => exchange_github(&s.client, creds, &code, &redirect_uri).await,
        "google" => exchange_google(&s.client, creds, &code, &redirect_uri).await,
        _ => Err("provider_unavailable"),
    };
    let email = match email {
        Ok(e) => auth::normalise_email(&e),
        Err(code) => {
            warn!("oauth {provider} exchange failed: {code}");
            return error_redirect(&app_url, code);
        }
    };

    // Resolve account: existing → sign in; new → create password-less account.
    let totp_enabled = match s.db.lock().await.find_user_by_email(&email) {
        Ok(Some(u)) => u.totp_enabled,
        Ok(None) => {
            // Random unguessable password so password login can never succeed.
            let pw = random_hex(24);
            let hash = match tokio::task::spawn_blocking(move || {
                bcrypt::hash(pw, bcrypt::DEFAULT_COST)
            })
            .await
            {
                Ok(Ok(h)) => h,
                _ => return error_redirect(&app_url, "oauth_failed"),
            };
            if let Err(e) = s
                .writer
                .create_oauth_user(email.clone(), provider.clone(), hash)
                .await
            {
                warn!("create_oauth_user error: {e}");
                return error_redirect(&app_url, "oauth_failed");
            }
            false
        }
        Err(e) => {
            warn!("oauth find_user error: {e}");
            return error_redirect(&app_url, "oauth_failed");
        }
    };

    // 2FA is enforced even for social login: hand back a partial token instead.
    if totp_enabled {
        let ver = auth::current_token_version(&s, &email).await;
        match auth::purpose_token(&email, auth::TokenPurpose::TwoFactorPartial, 300, &s.jwt_secret, ver)
        {
            Ok(t) => frontend_redirect(&app_url, &format!("partial_token={t}")),
            Err(e) => {
                warn!("oauth 2fa partial token error: {e}");
                error_redirect(&app_url, "oauth_failed")
            }
        }
    } else {
        let version =
            crate::rate_limit::resolve_token_version(&s.usage_counter, &s.db, &email).await;
        match auth::session_jwt(&email, &s.jwt_secret, version) {
            Ok(t) => frontend_redirect(&app_url, &format!("token={t}")),
            Err(e) => {
                warn!("oauth jwt error: {e}");
                error_redirect(&app_url, "oauth_failed")
            }
        }
    }
}

// ── Provider token exchange + verified-email fetch ──────────────────────────────

#[derive(Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
}

#[derive(Deserialize)]
struct GithubEmail {
    email: String,
    primary: bool,
    verified: bool,
}

async fn exchange_github(
    client: &reqwest::Client,
    creds: &ProviderCreds,
    code: &str,
    redirect_uri: &str,
) -> Result<String, &'static str> {
    let token: TokenResponse = client
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .header("User-Agent", USER_AGENT)
        .form(&[
            ("client_id", creds.client_id.as_str()),
            ("client_secret", creds.client_secret.as_str()),
            ("code", code),
            ("redirect_uri", redirect_uri),
        ])
        .send()
        .await
        .map_err(|_| "oauth_failed")?
        .json()
        .await
        .map_err(|_| "oauth_failed")?;

    let access = token.access_token.ok_or("oauth_failed")?;

    let emails: Vec<GithubEmail> = client
        .get("https://api.github.com/user/emails")
        .header("Authorization", format!("Bearer {access}"))
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", USER_AGENT)
        .send()
        .await
        .map_err(|_| "oauth_failed")?
        .json()
        .await
        .map_err(|_| "oauth_failed")?;

    // Prefer the primary verified address; otherwise any verified one.
    emails
        .iter()
        .find(|e| e.primary && e.verified)
        .or_else(|| emails.iter().find(|e| e.verified))
        .map(|e| e.email.clone())
        .ok_or("email_unverified")
}

#[derive(Deserialize)]
struct GoogleUserinfo {
    email: Option<String>,
    email_verified: Option<bool>,
}

async fn exchange_google(
    client: &reqwest::Client,
    creds: &ProviderCreds,
    code: &str,
    redirect_uri: &str,
) -> Result<String, &'static str> {
    let token: TokenResponse = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", creds.client_id.as_str()),
            ("client_secret", creds.client_secret.as_str()),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
        .map_err(|_| "oauth_failed")?
        .json()
        .await
        .map_err(|_| "oauth_failed")?;

    let access = token.access_token.ok_or("oauth_failed")?;

    let info: GoogleUserinfo = client
        .get("https://openidconnect.googleapis.com/v1/userinfo")
        .header("Authorization", format!("Bearer {access}"))
        .send()
        .await
        .map_err(|_| "oauth_failed")?
        .json()
        .await
        .map_err(|_| "oauth_failed")?;

    if info.email_verified != Some(true) {
        return Err("email_unverified");
    }
    info.email.ok_or("email_unverified")
}

#[cfg(test)]
mod state_binding_tests {
    use super::*;

    const SECRET: &str = "test-secret-not-used-anywhere-real";

    /// The attack this closes. An attacker starts a flow with their own account
    /// and gets a state token the server really did sign. They induce a victim's
    /// browser to load the callback with it. The victim's browser has no cookie
    /// from that flow, so the two halves disagree and the callback is refused.
    ///
    /// Before this, the signature was the whole check, and any anonymous caller
    /// could obtain a signature by requesting `start`.
    #[test]
    fn a_state_token_from_another_browser_does_not_verify() {
        let (attacker_state, attacker_nonce) = sign_state("github", SECRET).expect("mint");

        // The server still recognises its own signature. That was never the gap.
        let signed = verify_state(&attacker_state, "github", SECRET).expect("valid signature");
        assert_eq!(signed, attacker_nonce);

        // The victim's browser carries no cookie for this flow.
        let victim = axum::http::HeaderMap::new();
        assert_eq!(
            nonce_from_cookies(&victim),
            None,
            "a browser that never started a flow has no nonce to offer"
        );

        // A browser mid-flow of its own carries the wrong one.
        let (_, own_nonce) = sign_state("github", SECRET).expect("mint");
        assert!(
            !nonces_match(&signed, &own_nonce),
            "two flows must not share a nonce"
        );
    }

    /// The nonce has to survive the round trip, or every sign in breaks.
    #[test]
    fn the_nonce_a_flow_sets_is_the_nonce_it_reads_back() {
        let (_, nonce) = sign_state("google", SECRET).expect("mint");
        let cookie = state_cookie(&nonce, true);

        let mut headers = axum::http::HeaderMap::new();
        // What a browser sends back: the pairs only, no attributes.
        let sent = cookie.split(';').next().expect("name=value").to_string();
        headers.insert(
            axum::http::header::COOKIE,
            axum::http::HeaderValue::from_str(&sent).expect("header"),
        );

        assert_eq!(nonce_from_cookies(&headers).as_deref(), Some(nonce.as_str()));
    }

    /// The attributes are the security properties, so they are asserted rather
    /// than assumed. `Lax` and not `Strict`: the callback is a top level
    /// navigation from the provider's domain and `Strict` would strip the
    /// cookie from it, breaking every sign in.
    #[test]
    fn the_cookie_carries_the_attributes_that_make_it_safe() {
        let c = state_cookie("abc123", true);
        assert!(c.contains("HttpOnly"), "script must not read it: {c}");
        assert!(c.contains("SameSite=Lax"), "Strict breaks the callback: {c}");
        assert!(!c.contains("SameSite=Strict"), "{c}");
        assert!(c.contains("Secure"), "https deployments get Secure: {c}");
        assert!(c.contains("Path=/auth/oauth"), "scoped to the flow: {c}");
        assert!(
            c.contains(&format!("Max-Age={STATE_TTL_SECS}")),
            "it outlives no longer than the state token: {c}"
        );

        // A plain http origin drops Secure, because a Secure cookie is never
        // sent over http and sign in would fail with nothing to show for it.
        let insecure = state_cookie("abc123", false);
        assert!(!insecure.contains("Secure"), "{insecure}");
        assert!(insecure.contains("HttpOnly"), "{insecure}");
    }

    /// Spent on the way out, whatever happened, so one flow's nonce cannot be
    /// replayed into a second callback.
    #[test]
    fn clearing_expires_the_same_cookie_it_set() {
        let cleared = clear_state_cookie(true);
        assert!(cleared.starts_with(&format!("{STATE_COOKIE}=;")), "{cleared}");
        assert!(cleared.contains("Max-Age=0"), "{cleared}");
        // The path has to match the one it was set with, or the browser keeps
        // the original alongside the empty one.
        assert!(cleared.contains("Path=/auth/oauth"), "{cleared}");
        assert!(cleared.contains("HttpOnly"), "{cleared}");
    }

    /// Cookie parsing that matched on a prefix would accept a cookie somebody
    /// else set, which is the whole value of the check.
    #[test]
    fn a_cookie_whose_name_merely_starts_the_same_is_not_the_nonce() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            axum::http::HeaderValue::from_static(
                "astraeus_oauth_nonce_other=attacker; unrelated=1",
            ),
        );
        assert_eq!(nonce_from_cookies(&headers), None);

        // And it is found when it really is there, among others.
        headers.insert(
            axum::http::header::COOKIE,
            axum::http::HeaderValue::from_static(
                "unrelated=1; astraeus_oauth_nonce=real; other=2",
            ),
        );
        assert_eq!(nonce_from_cookies(&headers).as_deref(), Some("real"));

        // An empty value is no value.
        headers.insert(
            axum::http::header::COOKIE,
            axum::http::HeaderValue::from_static("astraeus_oauth_nonce=; other=2"),
        );
        assert_eq!(nonce_from_cookies(&headers), None);
    }

    /// A state token for one provider must not complete a flow for another.
    #[test]
    fn state_is_still_bound_to_its_provider() {
        let (token, _) = sign_state("github", SECRET).expect("mint");
        assert!(verify_state(&token, "github", SECRET).is_some());
        assert!(verify_state(&token, "google", SECRET).is_none());
        assert!(verify_state(&token, "github", "another-secret").is_none());
    }

    /// Secure follows the deployment's own scheme. It is read from the URL the
    /// provider redirects back to, because that is the origin the cookie has to
    /// survive on.
    #[test]
    fn secure_follows_the_deployment_scheme() {
        let cfg = |base: &str| OAuthConfig {
            github: None,
            google: None,
            redirect_base: base.to_string(),
        };
        assert!(cfg("https://astraeusio.com").cookies_are_secure());
        assert!(!cfg("http://localhost:3000").cookies_are_secure());
    }

    /// The handler itself, not its parts.
    ///
    /// A callback carrying a state token this server really signed, arriving at
    /// a browser that never began the flow, is refused with `bad_state`. The
    /// positive control matters as much: the same request *with* the matching
    /// cookie gets past the state check and fails later for a different reason,
    /// so the test cannot pass by rejecting everything.
    #[tokio::test]
    async fn a_callback_without_the_matching_cookie_is_refused() {
        use axum::extract::{Path as AxPath, Query, State};

        let client = reqwest::Client::new();
        let state = crate::routes::AppState::new(
            client.clone(),
            crate::db::Store::open(":memory:").expect("store"),
            crate::db_writer::spawn(
                crate::db::Store::open(":memory:").expect("writer store"),
                client,
            ),
            "http://ml".to_string(),
            SECRET.to_string(),
            None,
            "https://app.example".to_string(),
            OAuthConfig {
                github: None,
                google: None,
                redirect_base: "https://app.example".to_string(),
            },
        );

        let (token, nonce) = sign_state("github", SECRET).expect("mint");

        let call = |cookie: Option<String>| {
            let state = state.clone();
            let token = token.clone();
            async move {
                let mut headers = axum::http::HeaderMap::new();
                if let Some(c) = cookie {
                    headers.insert(
                        axum::http::header::COOKIE,
                        axum::http::HeaderValue::from_str(&c).expect("header"),
                    );
                }
                let resp = callback(
                    AxPath("github".to_string()),
                    Query(CallbackParams {
                        code: Some("any-code".to_string()),
                        state: Some(token),
                        error: None,
                    }),
                    State(state),
                    headers,
                )
                .await;
                let location = resp
                    .headers()
                    .get(axum::http::header::LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or_default()
                    .to_string();
                let cleared = resp
                    .headers()
                    .get(axum::http::header::SET_COOKIE)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or_default()
                    .to_string();
                (location, cleared)
            }
        };

        // No cookie: the victim's browser.
        let (loc, cleared) = call(None).await;
        assert!(
            loc.contains("error=bad_state"),
            "a callback with no state cookie must be refused, got {loc}"
        );
        assert!(
            cleared.contains("Max-Age=0"),
            "and the nonce is spent on the way out, got {cleared}"
        );

        // A cookie from some other flow.
        let (_, other_nonce) = sign_state("github", SECRET).expect("mint");
        let (loc, _) = call(Some(format!("{STATE_COOKIE}={other_nonce}"))).await;
        assert!(
            loc.contains("error=bad_state"),
            "a cookie from another flow must be refused, got {loc}"
        );

        // The matching cookie. No provider is configured in this state, so it
        // fails at the next step, which is the point: it got past the check.
        let (loc, _) = call(Some(format!("{STATE_COOKIE}={nonce}"))).await;
        assert!(
            !loc.contains("error=bad_state"),
            "the browser that began the flow must get past the state check, got {loc}"
        );
        assert!(
            loc.contains("error=provider_unavailable"),
            "and then fail for the reason this test state actually has, got {loc}"
        );
    }

    #[test]
    fn nonce_comparison_rejects_a_prefix_or_a_different_length() {
        assert!(nonces_match("abcdef", "abcdef"));
        assert!(!nonces_match("abcdef", "abcde"));
        assert!(!nonces_match("abcde", "abcdef"));
        assert!(!nonces_match("abcdef", "abcdeg"));
        assert!(!nonces_match("", "a"));
    }
}
