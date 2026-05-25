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
//!      full session token, or — if the account has TOTP enabled — a 2FA partial
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
}

fn random_hex(n_bytes: usize) -> String {
    use rand::RngCore;
    let mut buf = vec![0u8; n_bytes];
    rand::thread_rng().fill_bytes(&mut buf);
    hex::encode(buf)
}

fn sign_state(provider: &str, secret: &str) -> Result<String, jsonwebtoken::errors::Error> {
    let exp = (chrono::Utc::now().timestamp() + STATE_TTL_SECS) as u64;
    encode(
        &Header::default(),
        &StateClaims {
            provider: provider.to_string(),
            nonce: random_hex(16),
            exp,
        },
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

fn verify_state(token: &str, provider: &str, secret: &str) -> bool {
    decode::<StateClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map(|d| d.claims.provider == provider)
    .unwrap_or(false)
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

    let state = match sign_state(&provider, &s.jwt_secret) {
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
        Ok(u) => Redirect::to(u.as_str()).into_response(),
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

pub async fn callback(
    Path(provider): Path<String>,
    Query(params): Query<CallbackParams>,
    State(s): State<AppState>,
) -> Response {
    let app_url = s.app_url.clone();

    // User denied consent, or provider returned an error.
    if params.error.is_some() {
        return error_redirect(&app_url, "access_denied");
    }
    let (Some(code), Some(state)) = (params.code, params.state) else {
        return error_redirect(&app_url, "oauth_failed");
    };
    if !verify_state(&state, &provider, &s.jwt_secret) {
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
        Ok(e) => e.to_lowercase(),
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
        match auth::purpose_token(&email, "2fa_partial", 300, &s.jwt_secret) {
            Ok(t) => frontend_redirect(&app_url, &format!("partial_token={t}")),
            Err(e) => {
                warn!("oauth 2fa partial token error: {e}");
                error_redirect(&app_url, "oauth_failed")
            }
        }
    } else {
        match auth::session_jwt(&email, &s.jwt_secret) {
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
