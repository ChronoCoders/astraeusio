//! Retrying a poll that failed for a reason a second attempt can fix.
//!
//! `RETRY_COUNT` has been parsed, logged at startup and never read since the
//! poller was written, while `CLAUDE.md` and `README.md` both promised three
//! attempts with backoff. Production sets `RETRY_COUNT=3` and has been getting
//! one attempt. This is that promise made real, and deliberately narrower than
//! the promise was.
//!
//! What is retried is decided by the failure, not the call site:
//!
//! - a connect error, a timeout or a 5xx means the request never landed or the
//!   server said it could not serve it right now, so another attempt is worth
//!   making. NASA's APOD endpoint spent an evening alternating 503, connect
//!   timeout and 200, and lost an hour of data to every unlucky first attempt.
//! - a 404 is the retired IMF feed. Retrying that for forty days would have
//!   helped nobody and tripled the log while doing it. Neither would a 401 or a
//!   403, which are statements about credentials.
//! - a decode error is not retried either. A truncated body is transient and a
//!   changed schema is not, and `reqwest` cannot tell them apart. Both decode
//!   failures this codebase has actually suffered, the retired positional IMF
//!   feed and the NaN payload, were permanent.
//!
//! On 429 nothing is retried inside this poll cycle. `Retry-After` says how long
//! to wait, but `error_for_status` discards the response before the error
//! reaches here, so the header is not available to honour. Waiting for the next
//! scheduled poll respects the limit strictly, which is the point of a 429;
//! guessing a delay could hammer through a window we cannot see.
//!
//! Two bounds keep a retry from costing more than the poll it belongs to. The
//! per-attempt timeout is `min(HTTP_TIMEOUT, max(2s, interval))`, because a
//! 60 second timeout on a 5 second poller is a wedge waiting to happen with or
//! without retries. The budget for all attempts together is one poll interval,
//! so a missed poll stays cheaper than a stalled poller.

use std::fmt::Display;
use std::future::Future;
use std::time::Duration;

use tokio::time::Instant;
use tracing::{error, warn};

/// Floor for the per-attempt timeout, so a very fast poller still allows a
/// round trip that is merely slow rather than broken.
const MIN_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(2);

/// First backoff. Doubles per retry: 250ms, 500ms, 1s, 2s.
const BACKOFF_BASE: Duration = Duration::from_millis(250);

/// What a failure means for a second attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// The request never landed, or the server declined to serve it now.
    Transient,
    /// Rate limited. Not retried in this cycle; the next scheduled poll takes it.
    RateLimited,
    /// A second attempt cannot change the answer.
    Permanent,
}

/// Implemented by every error a poller can receive, so the decision lives with
/// the error type rather than being re-derived at each call site.
pub trait Classify {
    fn class(&self) -> Class;
}

/// Classification split from `reqwest::Error` so it can be tested directly.
/// Constructing the real error type is impractical; the mapping is the part
/// worth pinning.
pub fn class_from_parts(
    is_timeout: bool,
    is_connect: bool,
    is_request: bool,
    is_decode: bool,
    status: Option<u16>,
) -> Class {
    // A decode failure is about the payload, not the connection, so it is
    // checked first: the request plainly landed.
    if is_decode {
        return Class::Permanent;
    }
    if is_timeout || is_connect || is_request {
        return Class::Transient;
    }
    match status {
        Some(429) => Class::RateLimited,
        // 501 is a permanent statement about the endpoint, unlike its 5xx peers.
        Some(501) => Class::Permanent,
        Some(s) if (500..=599).contains(&s) => Class::Transient,
        _ => Class::Permanent,
    }
}

pub fn class_of(e: &reqwest::Error) -> Class {
    class_from_parts(
        e.is_timeout(),
        e.is_connect(),
        e.is_request(),
        e.is_decode(),
        e.status().map(|s| s.as_u16()),
    )
}

// Every poller error is classified here rather than in each source module, so
// the whole policy can be read in one place and one source cannot quietly
// disagree with the others about what a 404 means.

impl Classify for reqwest::Error {
    fn class(&self) -> Class {
        class_of(self)
    }
}

impl Classify for crate::noaa::NoaaError {
    fn class(&self) -> Class {
        match self {
            crate::noaa::NoaaError::Request(e) => class_of(e),
            // The payload arrived and was wrong. Asking again gets the same
            // wrong payload; this is the shape that froze the IMF table.
            crate::noaa::NoaaError::MissingField { .. }
            | crate::noaa::NoaaError::UnparseableField { .. }
            | crate::noaa::NoaaError::Json(_) => Class::Permanent,
        }
    }
}

impl Classify for crate::nasa::NasaError {
    fn class(&self) -> Class {
        match self {
            crate::nasa::NasaError::Request(e) => class_of(e),
            // A missing NASA_API_KEY is not going to appear on attempt two.
            crate::nasa::NasaError::Env(_) => Class::Permanent,
        }
    }
}

impl Classify for crate::iss::IssError {
    fn class(&self) -> Class {
        match self {
            crate::iss::IssError::Request(e) => class_of(e),
        }
    }
}

impl Classify for crate::astros::AstrosError {
    fn class(&self) -> Class {
        match self {
            crate::astros::AstrosError::Request(e) => class_of(e),
        }
    }
}

impl Classify for crate::starlink::StarlinkError {
    fn class(&self) -> Class {
        match self {
            crate::starlink::StarlinkError::Request(e) => class_of(e),
        }
    }
}

/// How hard to try, for one source.
#[derive(Debug, Clone)]
pub struct Policy {
    pub source: &'static str,
    /// Total attempts including the first, always at least 1.
    pub attempts: u32,
    /// Ceiling for all attempts together, one poll interval.
    pub budget: Duration,
    /// Ceiling for any single attempt.
    pub attempt_timeout: Duration,
}

impl Policy {
    pub fn new(
        source: &'static str,
        attempts: u32,
        interval_secs: u64,
        http_timeout_secs: u64,
    ) -> Self {
        let interval = Duration::from_secs(interval_secs.max(1));
        let http_timeout = Duration::from_secs(http_timeout_secs.max(1));
        Policy {
            source,
            attempts: attempts.max(1),
            budget: interval,
            // Never longer than the client's own timeout, never longer than one
            // interval, and never so short that a slow but healthy round trip
            // is cut off.
            attempt_timeout: http_timeout.min(interval.max(MIN_ATTEMPT_TIMEOUT)),
        }
    }
}

/// Runs `call` under the policy. Returns `None` when every attempt failed,
/// having already logged why, so a caller cannot forget to report it.
pub async fn run<T, E, F, Fut>(policy: &Policy, mut call: F) -> Option<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: Classify + Display,
{
    let started = Instant::now();
    let mut waited = Duration::ZERO;
    let mut attempt: u32 = 1;
    // Carried so a retried success can name the failure it survived.
    let mut last_error: Option<String> = None;

    loop {
        // Two failure shapes: the call returned an error, or it ran past the
        // per-attempt ceiling and was abandoned.
        let (class, text) = match tokio::time::timeout(policy.attempt_timeout, call()).await {
            Ok(Ok(value)) => {
                if attempt > 1 {
                    // The whole reason to log this at WARN. A retry that hides
                    // a degrading upstream trades one blindness for another, so
                    // a ridden-through failure names itself and the error it
                    // survived. Not ERROR: it succeeded, and the hourly poller
                    // check greps ERROR, which would then fire on every blip.
                    warn!(
                        source = policy.source,
                        attempt,
                        attempts = policy.attempts,
                        waited_ms = waited.as_millis() as u64,
                        "succeeded after retry: {}",
                        last_error.as_deref().unwrap_or("no detail recorded")
                    );
                }
                return Some(value);
            }
            Ok(Err(e)) => (e.class(), e.to_string()),
            Err(_) => (
                Class::Transient,
                format!("no response within {:?}", policy.attempt_timeout),
            ),
        };

        let redacted = crate::redact::secrets(&text).into_owned();

        if class == Class::RateLimited {
            error!(
                source = policy.source,
                attempt,
                rate_limited = true,
                "fetch: {redacted}; not retried, the next scheduled poll takes it"
            );
            return None;
        }
        if class == Class::Permanent || attempt >= policy.attempts {
            error!(
                source = policy.source,
                attempts = attempt,
                retryable = (class == Class::Transient),
                "fetch: {redacted}"
            );
            return None;
        }

        // Exponential, and only if the wait plus what has already elapsed still
        // leaves the poll inside its own interval.
        let backoff = BACKOFF_BASE * 2u32.pow(attempt - 1);
        if started.elapsed() + backoff >= policy.budget {
            error!(
                source = policy.source,
                attempts = attempt,
                budget_ms = policy.budget.as_millis() as u64,
                "fetch: {redacted}; giving up early, a retry would outlast the poll interval"
            );
            return None;
        }

        tokio::time::sleep(backoff).await;
        waited += backoff;
        attempt += 1;
        last_error = Some(redacted);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[derive(Debug)]
    struct Err_(Class, &'static str);
    impl Display for Err_ {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.1)
        }
    }
    impl Classify for Err_ {
        fn class(&self) -> Class {
            self.0
        }
    }

    fn policy(attempts: u32, interval: u64) -> Policy {
        Policy::new("test", attempts, interval, 60)
    }

    #[tokio::test(start_paused = true)]
    async fn a_transient_failure_is_retried_and_the_second_attempt_counts() {
        let calls = Cell::new(0u32);
        let got = run(&policy(3, 3600), || {
            calls.set(calls.get() + 1);
            let n = calls.get();
            async move {
                if n == 1 {
                    Err(Err_(Class::Transient, "503 Service Unavailable"))
                } else {
                    Ok("payload")
                }
            }
        })
        .await;
        assert_eq!(got, Some("payload"));
        assert_eq!(calls.get(), 2, "should have stopped as soon as it worked");
    }

    /// The IMF case. Retrying a 404 for forty days would have helped nobody.
    #[tokio::test(start_paused = true)]
    async fn a_permanent_failure_is_never_retried() {
        let calls = Cell::new(0u32);
        let got: Option<&str> = run(&policy(3, 3600), || {
            calls.set(calls.get() + 1);
            async { Err(Err_(Class::Permanent, "404 Not Found")) }
        })
        .await;
        assert!(got.is_none());
        assert_eq!(calls.get(), 1, "a 404 must cost exactly one request");
    }

    /// Respecting a rate limit means not sending more requests into it.
    #[tokio::test(start_paused = true)]
    async fn a_rate_limit_is_not_retried_inside_the_cycle() {
        let calls = Cell::new(0u32);
        let got: Option<&str> = run(&policy(5, 3600), || {
            calls.set(calls.get() + 1);
            async { Err(Err_(Class::RateLimited, "429 Too Many Requests")) }
        })
        .await;
        assert!(got.is_none());
        assert_eq!(calls.get(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn attempts_are_bounded_by_the_configured_count() {
        let calls = Cell::new(0u32);
        let got: Option<&str> = run(&policy(3, 3600), || {
            calls.set(calls.get() + 1);
            async { Err(Err_(Class::Transient, "connect timeout")) }
        })
        .await;
        assert!(got.is_none());
        assert_eq!(calls.get(), 3, "three attempts means three, not three retries");
    }

    #[tokio::test(start_paused = true)]
    async fn one_attempt_means_no_retry_at_all() {
        let calls = Cell::new(0u32);
        let got: Option<&str> = run(&policy(1, 3600), || {
            calls.set(calls.get() + 1);
            async { Err(Err_(Class::Transient, "503")) }
        })
        .await;
        assert!(got.is_none());
        assert_eq!(calls.get(), 1);
    }

    /// The ISS poller runs every five seconds. Retrying into the next poll is
    /// worse than missing this one.
    #[tokio::test(start_paused = true)]
    async fn the_budget_stops_a_retry_that_would_outlast_the_interval() {
        let calls = Cell::new(0u32);
        // A one second budget cannot fit the 250ms backoff plus an attempt that
        // burns its whole timeout.
        let p = Policy::new("poller/iss", 3, 1, 60);
        let got: Option<&str> = run(&p, || {
            calls.set(calls.get() + 1);
            async {
                tokio::time::sleep(Duration::from_millis(900)).await;
                Err(Err_(Class::Transient, "503"))
            }
        })
        .await;
        assert!(got.is_none());
        assert_eq!(calls.get(), 1, "the second attempt would have run past the interval");
    }

    /// A hung request must be abandoned, not allowed to wedge the loop.
    #[tokio::test(start_paused = true)]
    async fn an_attempt_that_hangs_is_abandoned_and_treated_as_transient() {
        let calls = Cell::new(0u32);
        let got = run(&policy(2, 3600), || {
            calls.set(calls.get() + 1);
            let n = calls.get();
            async move {
                if n == 1 {
                    // Never resolves. Under a paused clock the timeout fires.
                    std::future::pending::<()>().await;
                }
                Ok::<&str, Err_>("recovered")
            }
        })
        .await;
        assert_eq!(got, Some("recovered"));
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn the_per_attempt_timeout_never_exceeds_the_interval_or_the_client_timeout() {
        // ISS: five second interval must not carry a sixty second timeout.
        assert_eq!(
            Policy::new("poller/iss", 3, 5, 60).attempt_timeout,
            Duration::from_secs(5)
        );
        // Hourly: bounded by the client timeout, not the interval.
        assert_eq!(
            Policy::new("poller/apod", 3, 3600, 60).attempt_timeout,
            Duration::from_secs(60)
        );
        // A very fast interval still gets a floor, or nothing healthy completes.
        assert_eq!(
            Policy::new("fast", 3, 1, 60).attempt_timeout,
            MIN_ATTEMPT_TIMEOUT
        );
        // A short client timeout wins over a long interval.
        assert_eq!(
            Policy::new("poller/apod", 3, 3600, 5).attempt_timeout,
            Duration::from_secs(5)
        );
    }

    #[test]
    fn attempts_are_clamped_to_at_least_one() {
        assert_eq!(Policy::new("x", 0, 60, 60).attempts, 1);
    }

    #[test]
    fn transient_and_permanent_are_told_apart_by_status_and_kind() {
        let c = |s: u16| class_from_parts(false, false, false, false, Some(s));
        assert_eq!(c(503), Class::Transient);
        assert_eq!(c(500), Class::Transient);
        assert_eq!(c(502), Class::Transient);
        // Permanent statements about the endpoint or the caller.
        assert_eq!(c(501), Class::Permanent, "501 is not a passing condition");
        assert_eq!(c(404), Class::Permanent, "the retired IMF feed");
        assert_eq!(c(403), Class::Permanent);
        assert_eq!(c(401), Class::Permanent);
        assert_eq!(c(400), Class::Permanent);
        assert_eq!(c(429), Class::RateLimited);
        // Connection level failures, where no status exists at all.
        assert_eq!(
            class_from_parts(true, false, false, false, None),
            Class::Transient
        );
        assert_eq!(
            class_from_parts(false, true, false, false, None),
            Class::Transient
        );
        // A decode failure landed; the payload is the problem.
        assert_eq!(
            class_from_parts(false, false, false, true, Some(200)),
            Class::Permanent
        );
        // Decode wins even when the connection flags are also set, because the
        // response plainly arrived.
        assert_eq!(
            class_from_parts(true, false, false, true, None),
            Class::Permanent
        );
    }
}
