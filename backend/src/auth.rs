use axum::{
    Json,
    extract::{FromRequestParts, Path, State},
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Response},
};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use totp_rs::{Algorithm, Secret, TOTP};
use tracing::warn;

use crate::{db::DbError, db_writer::WriteCmd, mailer, rate_limit, routes::AppState};

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum AuthType {
    #[default]
    Jwt,
    ApiKey,
}

/// Audience of a session token. Held in the signed claims, so a token minted
/// for any other audience cannot be replayed as a session.
pub(crate) const AUD_SESSION: &str = "astraeus:session";

/// What a short lived token was minted to do. One secret signs every token this
/// service issues, so the audience inside the claims is the only thing that
/// separates them. Before this existed, a 2FA partial token, an email
/// verification token and a password reset token were all accepted as full
/// session tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TokenPurpose {
    TwoFactorPartial,
    VerifyEmail,
    ResetPassword,
}

impl TokenPurpose {
    pub(crate) const fn aud(self) -> &'static str {
        match self {
            Self::TwoFactorPartial => "astraeus:2fa_partial",
            Self::VerifyEmail => "astraeus:verify_email",
            Self::ResetPassword => "astraeus:reset_password",
        }
    }
}

/// Addresses are stored and compared in lower case, everywhere.
///
/// They were not, except on the OAuth path, which lowercased. So `Alice@x.com`
/// registering after `alice@x.com` created a second account, and a reset issued
/// for one casing never reached the other row (AUD-019). Mail domains are case
/// insensitive and no real provider treats the local part otherwise.
pub(crate) fn normalise_email(raw: &str) -> String {
    raw.trim().to_lowercase()
}

/// The password rule, applied identically wherever a password is set.
///
/// Length only, no composition rules. A rule demanding a symbol and a digit
/// buys predictable substitutions rather than entropy, which is why NIST
/// stopped recommending them; length is the property that reliably helps.
///
/// Twelve rather than eight. Eight was the floor at `change_password` and
/// `reset_password` while `register` enforced nothing at all, so an account
/// could be created with a password it could never later be changed to
/// (AUD-016). Raising the floor while fixing the split costs nothing: existing
/// passwords keep working and only a new one has to clear it.
///
/// The upper bound is not arbitrary either. bcrypt hashes at most 72 bytes and
/// silently ignores the rest, so a 100 character passphrase has 28 characters
/// that do nothing, and two passphrases sharing a 72 byte prefix both open the
/// account. Refusing is honest; truncating quietly is not.
pub(crate) const MIN_PASSWORD_BYTES: usize = 12;
pub(crate) const MAX_PASSWORD_BYTES: usize = 72;

pub(crate) fn validate_password(password: &str) -> Result<(), &'static str> {
    if password.len() < MIN_PASSWORD_BYTES {
        return Err("password must be at least 12 characters");
    }
    if password.len() > MAX_PASSWORD_BYTES {
        return Err("password must be at most 72 bytes, which is what bcrypt hashes");
    }
    Ok(())
}

/// A structural check, not an RFC 5322 parser and not a deliverability check.
///
/// It rejects what cannot be an address rather than trying to decide what is
/// one: the only proof an address works is mail arriving at it, which the
/// verification flow already does. `register` previously accepted anything,
/// including the empty string.
pub(crate) fn validate_email(email: &str) -> Result<(), &'static str> {
    const MAX_EMAIL_BYTES: usize = 254; // RFC 5321 path limit
    if email.is_empty() || email.len() > MAX_EMAIL_BYTES {
        return Err("email address is not a valid length");
    }
    if email.chars().any(char::is_whitespace) {
        return Err("email address must not contain spaces");
    }
    let Some((local, domain)) = email.split_once('@') else {
        return Err("email address must contain @");
    };
    if local.is_empty() || domain.is_empty() || domain.contains('@') {
        return Err("email address is not valid");
    }
    if !domain.contains('.') || domain.starts_with('.') || domain.ends_with('.') {
        return Err("email domain is not valid");
    }
    Ok(())
}

/// Validation that accepts exactly one audience and refuses a token that omits
/// `aud`, `exp` or `sub`. `Validation::default` checks the signature and `exp`
/// and nothing else, which is why any token signed with the shared secret used
/// to be interchangeable.
pub(crate) fn validation_for(aud: &str) -> Validation {
    let mut v = Validation::default();
    v.set_audience(&[aud]);
    v.set_required_spec_claims(&["exp", "aud", "sub"]);
    v
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthClaims {
    pub sub: String,
    pub exp: u64,
    pub aud: String,
    /// `users.token_version` at the moment this token was minted. A password
    /// change bumps the stored value, so tokens carrying an older one stop
    /// validating instead of outliving the change for their full 24 hours.
    pub ver: i64,
    #[serde(skip)]
    pub auth_type: AuthType,
}

/// Short lived token carrying the audience of the one thing it may be used for.
#[derive(Serialize, Deserialize)]
struct PurposeClaims {
    sub: String,
    exp: u64,
    aud: String,
    /// `users.token_version` when the link was minted. A password reset bumps
    /// it, so following the same link twice fails the second time instead of
    /// staying live for the rest of its hour (AUD-018).
    ///
    /// Note what this removes. Before it existed, a purpose token could not
    /// deserialize into `AuthClaims` because that struct has `ver` and this one
    /// did not, which accidentally blocked replaying a reset link as a session.
    /// That barrier is gone now, and the deliberate one, the audience check, is
    /// what remains. `a_token_that_differs_only_in_audience_is_rejected` exists
    /// precisely because the accident was doing the work.
    ver: i64,
}

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Deserialize)]
pub struct ForgotPasswordRequest {
    pub email: String,
}

#[derive(Deserialize)]
pub struct ResetPasswordRequest {
    pub token: String,
    pub new_password: String,
}

#[derive(Deserialize)]
pub struct TotpCodeRequest {
    pub code: String,
}

#[derive(Deserialize)]
pub struct TotpLoginRequest {
    pub partial_token: String,
    pub code: String,
}

#[derive(Serialize)]
struct LoginResponse {
    token: String,
}

// ── Token helpers ─────────────────────────────────────────────────────────────

pub(crate) fn purpose_token(
    sub: &str,
    purpose: TokenPurpose,
    ttl: i64,
    secret: &str,
    ver: i64,
) -> Result<String, jsonwebtoken::errors::Error> {
    let exp = (chrono::Utc::now().timestamp() + ttl) as u64;
    encode(
        &Header::default(),
        &PurposeClaims {
            sub: sub.to_string(),
            exp,
            aud: purpose.aud().to_string(),
            ver,
        },
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

/// The account's current token version, or 0 when it cannot be read.
///
/// Zero is the value a fresh account carries, so a link minted against a
/// failed read is refused rather than accepted: the check below compares
/// against the stored value, which is 1 or more for any account whose password
/// has ever changed.
pub(crate) async fn current_token_version(s: &AppState, email: &str) -> i64 {
    s.db.lock()
        .await
        .get_token_version(email)
        .unwrap_or_default()
}

/// Decodes a purpose token and refuses one minted before the account's
/// authentication last changed.
///
/// One function rather than a check at each call site, because a rule written
/// into one of three call sites is how the anomaly feed leaked for three weeks.
async fn decode_purpose_checked(
    s: &AppState,
    token: &str,
    purpose: TokenPurpose,
) -> Result<String, &'static str> {
    let (email, ver) = decode_purpose(token, purpose, &s.jwt_secret)?;
    if ver != current_token_version(s, &email).await {
        return Err("link has already been used or is no longer valid");
    }
    Ok(email)
}

fn decode_purpose(
    token: &str,
    purpose: TokenPurpose,
    secret: &str,
) -> Result<(String, i64), &'static str> {
    let claims = decode::<PurposeClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation_for(purpose.aud()),
    )
    .map_err(|_| "invalid or expired token")?
    .claims;
    Ok((claims.sub, claims.ver))
}

// ── TOTP helpers ──────────────────────────────────────────────────────────────

fn build_totp(secret_b32: &str, account: &str) -> Result<TOTP, &'static str> {
    let bytes = Secret::Encoded(secret_b32.to_string())
        .to_bytes()
        .map_err(|_| "invalid totp secret")?;
    TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        bytes,
        Some("Astraeusio".to_string()),
        account.to_string(),
    )
    .map_err(|_| "totp construction failed")
}

fn check_totp(secret_b32: &str, account: &str, code: &str) -> Result<bool, &'static str> {
    let totp = build_totp(secret_b32, account)?;
    totp.check_current(code).map_err(|_| "system time error")
}

// ── Handlers ──────────────────────────────────────────────────────────────────

pub async fn register(State(s): State<AppState>, Json(body): Json<RegisterRequest>) -> Response {
    // The same two rules `change_password` and `reset_password` apply. They ran
    // there and not here, so an account could be created with a password it
    // could never be changed to (AUD-016), and with an address that was not an
    // address at all.
    let email = normalise_email(&body.email);
    if let Err(reason) = validate_email(&email) {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": reason })),
        )
            .into_response();
    }
    if let Err(reason) = validate_password(&body.password) {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": reason })),
        )
            .into_response();
    }
    let password = body.password;
    let hash =
        match tokio::task::spawn_blocking(move || bcrypt::hash(password, bcrypt::DEFAULT_COST))
            .await
        {
            Ok(Ok(h)) => h,
            Ok(Err(e)) => {
                warn!("bcrypt hash error: {e}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "internal error" })),
                )
                    .into_response();
            }
            Err(e) => {
                warn!("spawn_blocking join error: {e}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "internal error" })),
                )
                    .into_response();
            }
        };

    match s.writer.create_user(email.clone(), hash).await {
        Ok(()) => {
            // Fire verification email if mailer is configured.
            if let Some(ref mc) = s.mailer
                && let Ok(token) =
                    purpose_token(&email, TokenPurpose::VerifyEmail, 86_400, &s.jwt_secret, 0)
            {
                let url = format!("{}/verify-email?token={}", s.app_url, token);
                let mc = mc.clone();
                tokio::spawn(async move {
                    mailer::send_verification_email(&mc, &email, &url).await;
                });
            }
            StatusCode::CREATED.into_response()
        }
        Err(DbError::EmailTaken) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "email already registered" })),
        )
            .into_response(),
        Err(e) => {
            warn!("create_user error: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response()
        }
    }
}

pub async fn login(State(s): State<AppState>, Json(body): Json<LoginRequest>) -> Response {
    // Before the database read and before bcrypt. Verifying a hash at the
    // default cost is deliberately slow, so an unthrottled login endpoint is
    // both a guessing oracle and a way to saturate the blocking pool.
    // Normalised here as well as at registration, so the backoff counter and the
    // lookup cannot be split across two spellings of one account.
    let email = normalise_email(&body.email);
    if let Some(wait) = rate_limit::attempt_blocked_for(&s.login_failures, &email) {
        warn!(source = "auth/login", subject = %email, wait, "attempt refused, backing off");
        return rate_limit::too_many_attempts_response(wait);
    }

    let user = match s.db.lock().await.find_user_by_email(&email) {
        Ok(Some(u)) => u,
        Ok(None) => {
            rate_limit::record_failure(&s.login_failures, &email);
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "invalid credentials" })),
            )
                .into_response();
        }
        Err(e) => {
            warn!("find_user error: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
    };

    let password = body.password;
    let hash = user.password_hash.clone();
    let valid = match tokio::task::spawn_blocking(move || bcrypt::verify(password, &hash)).await {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            warn!("bcrypt verify error: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
        Err(e) => {
            warn!("spawn_blocking join error: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
    };

    if !valid {
        rate_limit::record_failure(&s.login_failures, &email);
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "invalid credentials" })),
        )
            .into_response();
    }

    // The password was correct. A 2FA account is not signed in yet, so its
    // record is cleared when the code is accepted rather than here.
    if !user.totp_enabled {
        rate_limit::clear_failures(&s.login_failures, &email);
    }

    // If 2FA is active, issue a short-lived partial token instead of a full JWT.
    if user.totp_enabled {
        let ver = current_token_version(&s, &user.email).await;
        match purpose_token(&user.email, TokenPurpose::TwoFactorPartial, 300, &s.jwt_secret, ver) {
            Ok(partial) => {
                return Json(serde_json::json!({ "requires_2fa": true, "partial_token": partial }))
                    .into_response();
            }
            Err(e) => {
                warn!("2fa partial token error: {e}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "internal error" })),
                )
                    .into_response();
            }
        }
    }

    issue_jwt(&s, &user.email).await
}

pub async fn login_2fa(State(s): State<AppState>, Json(body): Json<TotpLoginRequest>) -> Response {
    let email = match decode_purpose_checked(&s, &body.partial_token, TokenPurpose::TwoFactorPartial).await {
        Ok(e) => e,
        Err(e) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": e })),
            )
                .into_response();
        }
    };

    let user = match s.db.lock().await.find_user_by_email(&email) {
        Ok(Some(u)) => u,
        Ok(None) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "user not found" })),
            )
                .into_response();
        }
        Err(e) => {
            warn!("login_2fa find_user: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
    };

    // The stored secret is encrypted, so reading it needs the key.
    let secret = match s.db.lock().await.totp_secret(&user) {
        Ok(Some(v)) => v,
        Ok(None) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "2FA not configured" })),
            )
                .into_response();
        }
        Err(e) => {
            warn!(source = "auth/2fa", "could not read second factor: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
    };

    // A six digit code has about three valid values at any moment out of a
    // million, so an unthrottled endpoint can cover a real fraction of that
    // space inside the partial token's five minute window.
    if let Some(wait) = rate_limit::attempt_blocked_for(&s.login_failures, &email) {
        warn!(source = "auth/2fa", subject = %email, wait, "code attempt refused, backing off");
        return rate_limit::too_many_attempts_response(wait);
    }

    match check_totp(&secret, &email, &body.code) {
        Ok(true) => {
            rate_limit::clear_failures(&s.login_failures, &email);
            issue_jwt(&s, &email).await
        }
        Ok(false) => {
            rate_limit::record_failure(&s.login_failures, &email);
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "invalid code" })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

// ── Email verification ─────────────────────────────────────────────────────────

pub async fn verify_email(Path(token): Path<String>, State(s): State<AppState>) -> Response {
    let email = match decode_purpose_checked(&s, &token, TokenPurpose::VerifyEmail).await {
        Ok(e) => e,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": e })),
            )
                .into_response();
        }
    };

    match s.writer.set_email_verified(email.clone()).await {
        Ok(()) => {
            if let Some(ref mc) = s.mailer {
                let mc = mc.clone();
                let app_url = s.app_url.clone();
                tokio::spawn(async move {
                    mailer::send_welcome_email(&mc, &email, &app_url).await;
                });
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => {
            warn!("set_email_verified: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response()
        }
    }
}

pub async fn resend_verification(State(s): State<AppState>, claims: AuthClaims) -> Response {
    let Some(ref mc) = s.mailer else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "email service not configured" })),
        )
            .into_response();
    };

    let user = match s.db.lock().await.find_user_by_email(&claims.sub) {
        Ok(Some(u)) => u,
        _ => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
    };

    if user.email_verified {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "email already verified" })),
        )
            .into_response();
    }

    let ver = current_token_version(&s, &claims.sub).await;
    let token = match purpose_token(&claims.sub, TokenPurpose::VerifyEmail, 86_400, &s.jwt_secret, ver)
    {
        Ok(t) => t,
        Err(e) => {
            warn!("token gen error: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
    };

    let url = format!("{}/verify-email?token={}", s.app_url, token);

    // Awaited, not spawned. This endpoint used to return 204 the instant it had
    // queued the work, so a user whose mail never sent was told it had. That is
    // survivable while verification gates nothing and is not once it gates
    // anything: this mail is the only way back for an account that is locked
    // out, and "we sent it" has to mean the provider took it.
    if !mailer::send_verification_email(mc, &claims.sub, &url).await {
        return (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({
                "error": "verification_email_failed",
                "detail": "the email provider did not accept the message, nothing was sent",
            })),
        )
            .into_response();
    }

    StatusCode::NO_CONTENT.into_response()
}

// ── 2FA setup / verify / disable ─────────────────────────────────────────────

/// Turns the second factor on or off and drops the cached token version in the
/// same call.
///
/// One function rather than two statements at each of two call sites. The
/// extractor caches `token_version` in the usage counter entry beside the plan,
/// so a bump that nobody tells the cache about does nothing until the entry
/// expires: `a_stale_cache_entry_would_defeat_the_invalidation` is the test that
/// pins exactly that. A plan change already clears the entry and a password
/// change was made to, which made 2FA the third writer to the same entry, and a
/// third call site remembering a second statement is where this goes wrong.
/// Pairing the write with the clear here means the pairing cannot be forgotten
/// by whatever writes the fourth.
async fn set_second_factor(s: &AppState, email: String, enabled: bool) -> Result<(), DbError> {
    let result = if enabled {
        s.writer.enable_totp(email.clone()).await
    } else {
        s.writer.disable_totp(email.clone()).await
    };
    if result.is_ok() {
        rate_limit::clear_user_cache(&s.usage_counter, &email);
    }
    result
}

pub async fn setup_2fa(State(s): State<AppState>, claims: AuthClaims) -> Response {
    let user = match s.db.lock().await.find_user_by_email(&claims.sub) {
        Ok(Some(u)) => u,
        _ => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
    };

    if user.totp_enabled {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "2FA already enabled" })),
        )
            .into_response();
    }

    // Generate random secret and build TOTP.
    let raw_secret = Secret::generate_secret();
    let secret_b32 = match raw_secret.to_encoded() {
        Secret::Encoded(s) => s,
        _ => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "secret encoding failed" })),
            )
                .into_response();
        }
    };

    let secret_bytes = match Secret::Encoded(secret_b32.clone()).to_bytes() {
        Ok(b) => b,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
    };

    let totp = match TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret_bytes,
        Some("Astraeusio".to_string()),
        claims.sub.clone(),
    ) {
        Ok(t) => t,
        Err(e) => {
            warn!("totp new error: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
    };

    let qr_base64 = match totp.get_qr_base64() {
        Ok(q) => q,
        Err(e) => {
            warn!("qr gen error: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
    };

    // Store pending (unconfirmed) secret.
    if let Err(e) = s
        .writer
        .set_totp_secret(claims.sub, secret_b32.clone())
        .await
    {
        // Without a key the secret cannot be stored encrypted, and storing it
        // in the clear is the thing this change exists to stop. Refuse, and say
        // so plainly rather than reporting an internal error.
        if matches!(e, DbError::EncryptionUnavailable) {
            warn!(
                source = "auth/2fa",
                "refusing 2FA setup: TOTP_ENCRYPTION_KEY is not configured"
            );
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "totp_unavailable",
                    "message": "Two factor sign in is not available yet. Contact support.",
                })),
            )
                .into_response();
        }
        warn!("set_totp_secret: {e}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "internal error" })),
        )
            .into_response();
    }

    Json(serde_json::json!({
        "secret":  secret_b32,
        "qr_code": format!("data:image/png;base64,{}", qr_base64),
    }))
    .into_response()
}

pub async fn verify_2fa(
    State(s): State<AppState>,
    claims: AuthClaims,
    Json(body): Json<TotpCodeRequest>,
) -> Response {
    let user = match s.db.lock().await.find_user_by_email(&claims.sub) {
        Ok(Some(u)) => u,
        _ => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
    };

    if user.totp_enabled {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "2FA already enabled" })),
        )
            .into_response();
    }

    // The stored secret is encrypted, so reading it needs the key.
    let secret = match s.db.lock().await.totp_secret(&user) {
        Ok(Some(v)) => v,
        Ok(None) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "2FA setup not initiated" })),
            )
                .into_response();
        }
        Err(e) => {
            warn!(source = "auth/2fa", "could not read second factor: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
    };

    match check_totp(&secret, &claims.sub, &body.code) {
        Ok(true) => {}
        Ok(false) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "invalid code" })),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e })),
            )
                .into_response();
        }
    }

    match set_second_factor(&s, claims.sub, true).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            warn!("enable_totp: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response()
        }
    }
}

pub async fn disable_2fa(
    State(s): State<AppState>,
    claims: AuthClaims,
    Json(body): Json<TotpCodeRequest>,
) -> Response {
    let user = match s.db.lock().await.find_user_by_email(&claims.sub) {
        Ok(Some(u)) => u,
        _ => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
    };

    if !user.totp_enabled {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "2FA is not enabled" })),
        )
            .into_response();
    }

    // The stored secret is encrypted, so reading it needs the key.
    let secret = match s.db.lock().await.totp_secret(&user) {
        Ok(Some(v)) => v,
        Ok(None) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
        Err(e) => {
            warn!(source = "auth/2fa", "could not read second factor: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
    };

    match check_totp(&secret, &claims.sub, &body.code) {
        Ok(true) => {}
        Ok(false) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "invalid code" })),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e })),
            )
                .into_response();
        }
    }

    match set_second_factor(&s, claims.sub, false).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            warn!("disable_totp: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response()
        }
    }
}

// ── Change password ───────────────────────────────────────────────────────────

pub async fn change_password(
    State(s): State<AppState>,
    claims: AuthClaims,
    Json(body): Json<ChangePasswordRequest>,
) -> Response {
    if let Err(reason) = validate_password(&body.new_password) {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": reason })),
        )
            .into_response();
    }

    let user = match s.db.lock().await.find_user_by_email(&claims.sub) {
        Ok(Some(u)) => u,
        Ok(None) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "user not found" })),
            )
                .into_response();
        }
        Err(e) => {
            warn!("change_password find_user error: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response();
        }
    };

    let current = body.current_password;
    let stored_hash = user.password_hash.clone();
    let valid =
        match tokio::task::spawn_blocking(move || bcrypt::verify(current, &stored_hash)).await {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                warn!("change_password bcrypt verify error: {e}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "internal error" })),
                )
                    .into_response();
            }
            Err(e) => {
                warn!("change_password spawn_blocking error: {e}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "internal error" })),
                )
                    .into_response();
            }
        };

    if !valid {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "current password is incorrect" })),
        )
            .into_response();
    }

    let new_pw = body.new_password;
    let new_hash =
        match tokio::task::spawn_blocking(move || bcrypt::hash(new_pw, bcrypt::DEFAULT_COST)).await
        {
            Ok(Ok(h)) => h,
            Ok(Err(e)) => {
                warn!("change_password bcrypt hash error: {e}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "internal error" })),
                )
                    .into_response();
            }
            Err(e) => {
                warn!("change_password spawn_blocking hash error: {e}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "internal error" })),
                )
                    .into_response();
            }
        };

    // The version bump happens in the same UPDATE as the hash, but the cached
    // copy beside the plan must go too, or the old value keeps validating the
    // very tokens this is meant to invalidate.
    let subject = claims.sub.clone();
    match s.writer.update_password(claims.sub, new_hash).await {
        Ok(()) => {
            rate_limit::clear_user_cache(&s.usage_counter, &subject);
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => {
            warn!("change_password update error: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response()
        }
    }
}

// ── Forgot / reset password ───────────────────────────────────────────────────

pub async fn forgot_password(
    State(s): State<AppState>,
    Json(body): Json<ForgotPasswordRequest>,
) -> Response {
    // Always 204 - never reveal whether the email exists.
    let requested = normalise_email(&body.email);
    let ver = current_token_version(&s, &requested).await;
    if let Some(ref mc) = s.mailer
        && let Ok(Some(_)) = s.db.lock().await.find_user_by_email(&requested)
        && let Ok(token) =
            purpose_token(&requested, TokenPurpose::ResetPassword, 3_600, &s.jwt_secret, ver)
    {
        let url = format!("{}/reset-password?token={}", s.app_url, token);
        let mc = mc.clone();
        let email = requested.clone();
        tokio::spawn(async move {
            mailer::send_password_reset_email(&mc, &email, &url).await;
        });
    }
    StatusCode::NO_CONTENT.into_response()
}

pub async fn reset_password(
    State(s): State<AppState>,
    Json(body): Json<ResetPasswordRequest>,
) -> Response {
    if let Err(reason) = validate_password(&body.new_password) {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": reason })),
        )
            .into_response();
    }

    let email = match decode_purpose_checked(&s, &body.token, TokenPurpose::ResetPassword).await {
        Ok(e) => e,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": e })),
            )
                .into_response();
        }
    };

    let new_pw = body.new_password;
    let new_hash =
        match tokio::task::spawn_blocking(move || bcrypt::hash(new_pw, bcrypt::DEFAULT_COST)).await
        {
            Ok(Ok(h)) => h,
            Ok(Err(e)) => {
                warn!("reset_password bcrypt error: {e}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "internal error" })),
                )
                    .into_response();
            }
            Err(e) => {
                warn!("reset_password spawn_blocking error: {e}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "internal error" })),
                )
                    .into_response();
            }
        };

    let subject = email.clone();
    match s.writer.update_password(email, new_hash).await {
        Ok(()) => {
            rate_limit::clear_user_cache(&s.usage_counter, &subject);
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => {
            warn!("reset_password update error: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response()
        }
    }
}

// ── JWT / API-key extractor ────────────────────────────────────────────────────

fn sha256_hex(input: &str) -> String {
    Sha256::digest(input.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Mint a 24h session JWT string (the token returned on successful login).
pub(crate) fn session_jwt(
    email: &str,
    secret: &str,
    token_version: i64,
) -> Result<String, jsonwebtoken::errors::Error> {
    let exp = (chrono::Utc::now().timestamp() + 86_400) as u64;
    let claims = AuthClaims {
        sub: email.to_string(),
        exp,
        aud: AUD_SESSION.to_string(),
        ver: token_version,
        auth_type: AuthType::Jwt,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

async fn issue_jwt(s: &AppState, email: &str) -> Response {
    let version = rate_limit::resolve_token_version(&s.usage_counter, &s.db, email).await;
    match session_jwt(email, &s.jwt_secret, version) {
        Ok(token) => Json(LoginResponse { token }).into_response(),
        Err(e) => {
            warn!("jwt encode error: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            )
                .into_response()
        }
    }
}

impl FromRequestParts<AppState> for AuthClaims {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or_else(|| {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({ "error": "missing or invalid Authorization header" })),
                )
                    .into_response()
            })?;

        // API key path - prefix "ast_"
        if token.starts_with("ast_") {
            let hash = sha256_hex(token);
            let sub_opt = {
                let db = state.db.lock().await;
                db.find_api_key_by_hash(&hash).map_err(|e| {
                    warn!("api_key lookup error: {e}");
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({ "error": "internal error" })),
                    )
                        .into_response()
                })?
            };
            if sub_opt.is_some() {
                state.writer.fire(WriteCmd::TouchApiKey(hash));
            }

            match sub_opt {
                Some(sub) => {
                    rate_limit::check_and_increment(&state.usage_counter, &state.db, &sub).await?;
                    return Ok(AuthClaims {
                        sub,
                        exp: u64::MAX,
                        aud: AUD_SESSION.to_string(),
                        // An API key is not a session, so no version applies.
                        ver: 0,
                        auth_type: AuthType::ApiKey,
                    });
                }
                None => {
                    return Err((
                        StatusCode::UNAUTHORIZED,
                        Json(serde_json::json!({ "error": "invalid API key" })),
                    )
                        .into_response());
                }
            }
        }

        // JWT path. Only a token minted with the session audience is a session.
        // A 2FA partial, an email verification link or a password reset link is
        // signed with the same secret and would otherwise be accepted here.
        let claims = decode::<AuthClaims>(
            token,
            &DecodingKey::from_secret(state.jwt_secret.as_bytes()),
            &validation_for(AUD_SESSION),
        )
        .map(|data| data.claims)
        .map_err(|e| {
            warn!(source = "auth", "session token rejected: {e}");
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "invalid or expired token" })),
            )
                .into_response()
        })?;

        // Session requests are deliberately not counted. The quota is the thing
        // being sold and the dashboard is the product, so a user browsing their
        // own dashboard must not spend the allowance they bought for the API.
        // Sessions remain subject to the failed sign in backoff, and to whatever
        // abuse limit the edge grows later.
        //
        // If a paid dashboard tier ever makes this the wrong call, the change is
        // one line here plus a second counter: give AuthType::Jwt its own key
        // space, for example `session:{email}`, so dashboard usage and API usage
        // accrue separately and each can carry its own limit. Counting both into
        // one bucket is the thing to avoid, because then the two products
        // compete for the same allowance.

        // Reject a token minted before the account's current version. The
        // lookup is cached beside the plan, so this costs a map hit on the warm
        // path.
        let current = rate_limit::resolve_token_version(
            &state.usage_counter,
            &state.db,
            &claims.sub,
        )
        .await;
        if claims.ver != current {
            warn!(
                source = "auth",
                subject = %claims.sub,
                token_version = claims.ver,
                current_version = current,
                "session token predates a credential change"
            );
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "invalid or expired token" })),
            )
                .into_response());
        }

        Ok(claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;

    const SECRET: &str = "test-secret-not-used-anywhere-real";

    fn test_state() -> AppState {
        let db = crate::db::Store::open(":memory:").expect("in-memory store");
        let client = reqwest::Client::new();
        let writer = crate::db_writer::spawn(
            crate::db::Store::open(":memory:").expect("writer store"),
            client.clone(),
        );
        AppState::new(
            client,
            db,
            writer,
            "http://ml".to_string(),
            SECRET.to_string(),
            None,
            "http://app".to_string(),
            crate::oauth::OAuthConfig {
                github: None,
                google: None,
                redirect_base: "http://app".to_string(),
            },
        )
    }

    /// Runs a bearer token through the real session extractor.
    async fn extract(state: &AppState, token: &str) -> Result<AuthClaims, StatusCode> {
        let req = Request::builder()
            .header("Authorization", format!("Bearer {token}"))
            .body(())
            .expect("request");
        let (mut parts, ()) = req.into_parts();
        AuthClaims::from_request_parts(&mut parts, state)
            .await
            .map_err(|resp| resp.status())
    }

    const EVERY_PURPOSE: [TokenPurpose; 3] = [
        TokenPurpose::TwoFactorPartial,
        TokenPurpose::VerifyEmail,
        TokenPurpose::ResetPassword,
    ];

    /// A 2FA partial, an email verification link and a password reset link are
    /// all signed with the same secret as a session. Each one used to be a
    /// working session token, which made TOTP a formality and turned every
    /// verification email into a bearer credential for its whole lifetime.
    #[tokio::test]
    async fn purpose_tokens_are_rejected_by_the_session_extractor() {
        let state = test_state();
        for purpose in EVERY_PURPOSE {
            let token =
                purpose_token("user@example.com", purpose, 300, SECRET, 0).expect("mint");
            let got = extract(&state, &token).await;
            assert_eq!(
                got.err(),
                Some(StatusCode::UNAUTHORIZED),
                "{} must not be accepted as a session",
                purpose.aud()
            );
        }
    }

    /// The test above asserts the property. This one asserts the mechanism.
    ///
    /// Mutation testing on 2026-08-31 found that removing the audience check
    /// from the session extractor broke nothing: a purpose token is also
    /// rejected because `PurposeClaims` has no `ver` field, which `AuthClaims`
    /// gained months later for a different finding, so it fails to deserialize
    /// whatever the audience says. The property was held by an accident of
    /// claim shape rather than by the check written to hold it, and a refactor
    /// that aligned the two structs would have removed the real defence with
    /// the suite still green.
    ///
    /// So this token is a session token byte for byte except for one field. It
    /// deserializes into `AuthClaims` cleanly, carries a token version the
    /// extractor can resolve, and is signed with the right secret. The audience
    /// is the only thing left that can reject it, which makes this test fail if
    /// and only if the audience check is gone.
    #[tokio::test]
    async fn a_token_that_differs_only_in_audience_is_rejected() {
        let state = test_state();
        let exp = (chrono::Utc::now().timestamp() + 300) as u64;

        for aud in [
            TokenPurpose::TwoFactorPartial.aud(),
            TokenPurpose::VerifyEmail.aud(),
            TokenPurpose::ResetPassword.aud(),
            // oauth.rs keeps this one private, so it is spelled out here. The
            // start endpoint is unauthenticated and hands it to any caller.
            "astraeus:oauth_state",
            "astraeus:something_invented_later",
            "",
        ] {
            let token = encode(
                &Header::default(),
                &AuthClaims {
                    sub: "user@example.com".to_string(),
                    exp,
                    aud: aud.to_string(),
                    ver: 0,
                    auth_type: AuthType::Jwt,
                },
                &EncodingKey::from_secret(SECRET.as_bytes()),
            )
            .expect("mint");

            assert_eq!(
                extract(&state, &token).await.err(),
                Some(StatusCode::UNAUTHORIZED),
                "a session-shaped token with audience {aud:?} must not be accepted"
            );
        }
    }

    #[tokio::test]
    async fn a_session_token_is_accepted_by_the_session_extractor() {
        let state = test_state();
        let token = session_jwt("user@example.com", SECRET, 0).expect("mint");
        let claims = extract(&state, &token).await.expect("session accepted");
        assert_eq!(claims.sub, "user@example.com");
        assert_eq!(claims.aud, AUD_SESSION);
    }

    /// A reset link used once must not work twice.
    ///
    /// It stayed live for its full hour: the link carried no version, so
    /// nothing about following it made it stale. Anyone who saw the URL after
    /// the fact, in a shared inbox, a browser history, a proxy log, could set
    /// the password again (AUD-018).
    #[tokio::test]
    async fn a_used_reset_link_is_refused_the_second_time() {
        let state = test_state();
        let email = "reset@example.com";
        {
            let db = state.db.lock().await;
            db.create_user(email, "hash").expect("create user");
        }

        let ver = current_token_version(&state, email).await;
        let link = purpose_token(email, TokenPurpose::ResetPassword, 3_600, SECRET, ver)
            .expect("mint");

        assert_eq!(
            decode_purpose_checked(&state, &link, TokenPurpose::ResetPassword).await,
            Ok(email.to_string()),
            "the link must work the first time"
        );

        // Following it sets a password, which bumps the version.
        {
            let db = state.db.lock().await;
            db.update_password_hash(email, "new-hash").expect("set password");
        }

        assert!(
            decode_purpose_checked(&state, &link, TokenPurpose::ResetPassword)
                .await
                .is_err(),
            "the same link must not work again"
        );
    }

    /// Turning the second factor on must invalidate tokens minted before it.
    ///
    /// Somebody enables 2FA because they think a session is stolen. Before this,
    /// the thief kept a working token for up to twenty four hours: the
    /// countermeasure did not touch the thing it was taken against.
    #[tokio::test]
    async fn enabling_the_second_factor_invalidates_existing_sessions() {
        let state = test_state();
        let email = "totp@example.com";
        {
            let db = state.db.lock().await;
            db.create_user(email, "hash").expect("create user");
        }

        let old_token = session_jwt(email, SECRET, 0).expect("mint");
        extract(&state, &old_token).await.expect("valid before the change");

        {
            let db = state.db.lock().await;
            db.enable_totp(email).expect("enable");
            assert_eq!(
                db.get_token_version(email).expect("version"),
                1,
                "the flag and the version must move together"
            );
        }
        // set_second_factor pairs this with the write; do the same here.
        rate_limit::clear_user_cache(&state.usage_counter, email);

        assert_eq!(
            extract(&state, &old_token).await.err(),
            Some(StatusCode::UNAUTHORIZED),
            "a token minted before 2FA was enabled must be refused"
        );
    }

    /// One rule, wherever a password is set. `register` enforced nothing while
    /// the other two enforced eight characters, so an account could be created
    /// with a password it could never be changed to (AUD-016).
    #[test]
    fn the_password_rule_is_the_same_wherever_a_password_is_set() {
        assert!(validate_password("").is_err());
        assert!(validate_password("short").is_err());
        assert!(validate_password(&"a".repeat(MIN_PASSWORD_BYTES - 1)).is_err());
        assert!(validate_password(&"a".repeat(MIN_PASSWORD_BYTES)).is_ok());
        // bcrypt hashes 72 bytes and silently ignores the rest, so a longer one
        // would have characters that do nothing.
        assert!(validate_password(&"a".repeat(MAX_PASSWORD_BYTES)).is_ok());
        assert!(validate_password(&"a".repeat(MAX_PASSWORD_BYTES + 1)).is_err());
    }

    #[test]
    fn an_address_that_cannot_be_one_is_refused() {
        for good in ["a@b.co", "first.last@sub.example.com", "x+tag@example.co.uk"] {
            assert!(validate_email(good).is_ok(), "{good} should be accepted");
        }
        for bad in ["", "no-at-sign", "@example.com", "user@", "user@nodot",
                    "user@.example.com", "user@example.", "two@at@example.com",
                    "has space@example.com"] {
            assert!(validate_email(bad).is_err(), "{bad} should be refused");
        }
    }

    /// Addresses are folded before anything looks them up, so one account
    /// cannot become two by capitalisation, and a reset issued for one spelling
    /// reaches the row that exists (AUD-019).
    #[test]
    fn addresses_are_normalised_before_anything_looks_them_up() {
        for (raw, expected) in [
            ("Alice@Example.COM", "alice@example.com"),
            ("  alice@example.com  ", "alice@example.com"),
            ("ALICE@EXAMPLE.COM", "alice@example.com"),
        ] {
            assert_eq!(normalise_email(raw), expected);
        }
    }

    /// The other direction: a full session must not stand in for the short
    /// lived token a flow demands, so a stolen session cannot complete someone
    /// else's 2FA login or password reset.
    #[test]
    fn a_session_token_is_rejected_where_a_purpose_token_is_required() {
        let session = session_jwt("user@example.com", SECRET, 0).expect("mint");
        for purpose in EVERY_PURPOSE {
            assert!(
                decode_purpose(&session, purpose, SECRET).is_err(),
                "a session token must not satisfy {}",
                purpose.aud()
            );
        }
    }

    /// Each purpose token works for its own purpose and no other.
    #[test]
    fn a_purpose_token_is_accepted_only_for_its_own_purpose() {
        for minted in EVERY_PURPOSE {
            let token = purpose_token("user@example.com", minted, 300, SECRET, 0).expect("mint");
            assert_eq!(
                decode_purpose(&token, minted, SECRET).ok().map(|(sub, _)| sub).as_deref(),
                Some("user@example.com"),
                "{} must satisfy its own purpose",
                minted.aud()
            );
            for other in EVERY_PURPOSE {
                if other == minted {
                    continue;
                }
                assert!(
                    decode_purpose(&token, other, SECRET).is_err(),
                    "{} must not satisfy {}",
                    minted.aud(),
                    other.aud()
                );
            }
        }
    }

    /// The pre-fix token shape, `{sub, exp}` with no audience at all. Tokens
    /// already issued have this shape, so they must fail rather than be
    /// grandfathered in.
    #[tokio::test]
    async fn a_token_without_an_audience_is_rejected() {
        #[derive(Serialize)]
        struct Legacy {
            sub: String,
            exp: u64,
        }
        let token = encode(
            &Header::default(),
            &Legacy {
                sub: "user@example.com".to_string(),
                exp: (chrono::Utc::now().timestamp() + 300) as u64,
            },
            &EncodingKey::from_secret(SECRET.as_bytes()),
        )
        .expect("mint");
        let state = test_state();
        assert_eq!(extract(&state, &token).await.err(), Some(StatusCode::UNAUTHORIZED));
    }

    /// The exact 2FA partial token this service used to mint: `{sub, exp,
    /// purpose}`. On the old code this was accepted as a full session, which is
    /// how TOTP could be skipped entirely. Tokens of this shape are still in
    /// circulation until they expire.
    #[tokio::test]
    async fn the_pre_fix_partial_token_shape_is_rejected() {
        #[derive(Serialize)]
        struct LegacyPurpose {
            sub: String,
            exp: u64,
            purpose: String,
        }
        let token = encode(
            &Header::default(),
            &LegacyPurpose {
                sub: "user@example.com".to_string(),
                exp: (chrono::Utc::now().timestamp() + 300) as u64,
                purpose: "2fa_partial".to_string(),
            },
            &EncodingKey::from_secret(SECRET.as_bytes()),
        )
        .expect("mint");
        let state = test_state();
        assert_eq!(
            extract(&state, &token).await.err(),
            Some(StatusCode::UNAUTHORIZED),
            "the old partial token shape must not authenticate a session"
        );
    }

    /// The quota is what is sold and the dashboard is the product, so browsing
    /// the dashboard must not spend the API allowance. Well past the free
    /// limit of 100 a day, a session still serves and still counts nothing.
    #[tokio::test]
    async fn session_requests_do_not_touch_the_quota() {
        let state = test_state();
        let token = session_jwt("dashboard@example.com", SECRET, 0).expect("mint");
        let limit = crate::rate_limit::plan_limit("free").expect("free is capped");

        for _ in 0..limit + 5 {
            extract(&state, &token).await.expect("sessions are never throttled");
        }
        // The extractor caches the token version beside the plan, so an entry
        // does exist. What must stay at zero is the count: a session may be
        // known to the counter, it must never spend against it.
        let counted = state
            .usage_counter
            .get("dashboard@example.com")
            .map(|e| e.count)
            .unwrap_or(0);
        assert_eq!(counted, 0, "a session must never spend quota");
    }

    /// The API key path is the one that counts, and the limit still binds there.
    #[tokio::test]
    async fn api_key_requests_are_counted_and_capped() {
        let state = test_state();
        let key = "ast_testkey_0123456789";
        let email = "apiuser@example.com";
        {
            let db = state.db.lock().await;
            db.create_api_key("key-1", email, &sha256_hex(key), "test key", None)
                .expect("create key");
        }

        let limit = crate::rate_limit::plan_limit("free").expect("free is capped");
        for expected in 1..=3u64 {
            extract(&state, key).await.expect("accepted");
            let counted = state
                .usage_counter
                .get(email)
                .map(|e| e.count)
                .expect("counter entry");
            assert_eq!(counted, expected, "API key request {expected} must count");
        }

        for _ in 3..limit {
            extract(&state, key).await.expect("within the limit");
        }
        assert_eq!(
            extract(&state, key).await.err(),
            Some(StatusCode::TOO_MANY_REQUESTS),
            "the API key path must still be capped"
        );
    }

    /// A session minted before a password change must stop working. Without
    /// this a stolen token outlived a reset for the rest of its 24 hours.
    #[tokio::test]
    async fn a_password_change_invalidates_existing_sessions() {
        let state = test_state();
        let email = "rotate@example.com";
        {
            let db = state.db.lock().await;
            db.create_user(email, "irrelevant-hash").expect("create user");
            assert_eq!(db.get_token_version(email).expect("version"), 0);
        }

        let old_token = session_jwt(email, SECRET, 0).expect("mint");
        extract(&state, &old_token).await.expect("valid before the change");

        {
            let db = state.db.lock().await;
            db.update_password_hash(email, "new-hash").expect("change password");
            assert_eq!(
                db.get_token_version(email).expect("version"),
                1,
                "the hash and the version must move together"
            );
        }
        // The handler clears the cache; do the same here.
        rate_limit::clear_user_cache(&state.usage_counter, email);

        assert_eq!(
            extract(&state, &old_token).await.err(),
            Some(StatusCode::UNAUTHORIZED),
            "a token minted before the change must be refused"
        );

        let new_token = session_jwt(email, SECRET, 1).expect("mint");
        extract(&state, &new_token).await.expect("a fresh token works");
    }

    /// The cache is the hazard. A plan change already clears the entry; a
    /// password change must too, or the pre change version keeps being served
    /// and the invalidation does nothing at all.
    #[tokio::test]
    async fn a_stale_cache_entry_would_defeat_the_invalidation() {
        let state = test_state();
        let email = "stale@example.com";
        {
            let db = state.db.lock().await;
            db.create_user(email, "irrelevant-hash").expect("create user");
        }

        let old_token = session_jwt(email, SECRET, 0).expect("mint");
        extract(&state, &old_token).await.expect("valid, and now cached");
        assert!(
            state.usage_counter.get(email).is_some(),
            "the lookup must have cached the version"
        );

        {
            let db = state.db.lock().await;
            db.update_password_hash(email, "new-hash").expect("change password");
        }

        // Deliberately skip clear_user_cache to show what it is preventing.
        extract(&state, &old_token)
            .await
            .expect("the stale cache still accepts the old token");

        // Clearing it, as both password paths do, closes the hole.
        rate_limit::clear_user_cache(&state.usage_counter, email);
        assert_eq!(
            extract(&state, &old_token).await.err(),
            Some(StatusCode::UNAUTHORIZED)
        );
    }

    /// A token signed with a different secret must fail whatever its audience.
    #[tokio::test]
    async fn a_foreign_signature_is_rejected() {
        let token = session_jwt("user@example.com", "some-other-secret", 0).expect("mint");
        let state = test_state();
        assert_eq!(extract(&state, &token).await.err(), Some(StatusCode::UNAUTHORIZED));
    }
}
