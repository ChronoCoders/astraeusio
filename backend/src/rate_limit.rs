use std::sync::Arc;

use crate::db::Store;
use crate::db_writer::{DbWriterHandle, WriteCmd};
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use dashmap::DashMap;
use tokio::sync::Mutex;

pub struct UsageEntry {
    pub count: u64,
    pub period_start: i64,
    pub plan: String,
    /// Cached `users.token_version`. Anything that invalidates sessions must
    /// remove the whole entry, or a stale value here defeats the check.
    pub token_version: i64,
}

pub type UsageCounter = DashMap<String, UsageEntry>;

pub fn plan_limit(plan: &str) -> Option<u64> {
    match plan {
        "free" => Some(100),
        "developer" => Some(10_000),
        "pro" => Some(100_000),
        "business" => Some(1_000_000),
        "enterprise" => None,
        _ => Some(100),
    }
}

fn is_daily(plan: &str) -> bool {
    matches!(plan, "free")
}

pub fn current_period_start(plan: &str, now: i64) -> i64 {
    if is_daily(plan) {
        now - (now % 86_400)
    } else {
        use chrono::{Datelike, TimeZone, Utc};
        Utc.timestamp_opt(now, 0)
            .single()
            .and_then(|dt| {
                Utc.with_ymd_and_hms(dt.year(), dt.month(), 1, 0, 0, 0)
                    .single()
            })
            .map(|d| d.timestamp())
            .unwrap_or(now)
    }
}

pub fn period_end(plan: &str, period_start: i64) -> i64 {
    if is_daily(plan) {
        period_start + 86_400
    } else {
        use chrono::{Datelike, TimeZone, Utc};
        Utc.timestamp_opt(period_start, 0)
            .single()
            .and_then(|dt| {
                let (year, month) = if dt.month() == 12 {
                    (dt.year() + 1, 1u32)
                } else {
                    (dt.year(), dt.month() + 1)
                };
                Utc.with_ymd_and_hms(year, month, 1, 0, 0, 0).single()
            })
            .map(|d| d.timestamp())
            .unwrap_or(period_start + 30 * 86_400)
    }
}

/// Check the in-memory counter; return Err(429 Response) if limit exceeded, else increment.
/// On first request or period rollover, fetches the user's plan from the database.
pub async fn check_and_increment(
    counter: &Arc<UsageCounter>,
    db: &Arc<Mutex<Store>>,
    email: &str,
) -> Result<(), Response> {
    let now_ts = chrono::Utc::now().timestamp();

    // Hot path: entry present and period still valid - check + increment under shard lock.
    if let Some(mut entry) = counter.get_mut(email) {
        let p_end = period_end(&entry.plan, entry.period_start);
        if now_ts < p_end {
            if let Some(limit) = plan_limit(&entry.plan)
                && entry.count >= limit
            {
                return Err(rate_limit_response(&entry.plan, limit, p_end));
            }
            entry.count += 1;
            return Ok(());
        }
        // Period expired - fall through; shard lock released here.
    }

    // Cold path: no entry yet or period rolled - fetch plan from DB.
    let plan = {
        let guard: tokio::sync::MutexGuard<'_, Store> = db.lock().await;
        guard
            .get_user_plan(email)
            .unwrap_or_else(|_| "free".to_string())
    };
    let p_start = current_period_start(&plan, now_ts);
    let p_end = period_end(&plan, p_start);

    // Insert or get existing entry under shard lock (no await below this point).
    let mut entry = counter
        .entry(email.to_string())
        .or_insert_with(|| UsageEntry {
            count: 0,
            period_start: p_start,
            plan: plan.clone(),
            token_version: 0,
        });

    // Reset if an existing entry belongs to a prior period.
    if entry.period_start < p_start {
        entry.count = 0;
        entry.period_start = p_start;
        entry.plan = plan.clone();
    }

    if let Some(limit) = plan_limit(&entry.plan)
        && entry.count >= limit
    {
        return Err(rate_limit_response(&entry.plan, limit, p_end));
    }
    entry.count += 1;
    Ok(())
}

fn rate_limit_response(plan: &str, limit: u64, reset_at: i64) -> Response {
    (
        StatusCode::TOO_MANY_REQUESTS,
        Json(serde_json::json!({
            "error":    "rate_limit_exceeded",
            "plan":     plan,
            "limit":    limit,
            "reset_at": reset_at,
        })),
    )
        .into_response()
}

/// Background task: flush in-memory counters to `usage_records` every 60 s.
pub fn spawn_flush_task(counter: Arc<UsageCounter>, writer: DbWriterHandle) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            let snapshots: Vec<(String, u64, i64, String)> = counter
                .iter()
                .map(|r| {
                    (
                        r.key().clone(),
                        r.value().count,
                        r.value().period_start,
                        r.value().plan.clone(),
                    )
                })
                .collect();
            for (email, count, period_start, plan) in snapshots {
                let p_end = period_end(&plan, period_start);
                writer.fire(WriteCmd::FlushUsage {
                    email,
                    count: count as i64,
                    period_start,
                    period_end: p_end,
                });
            }
        }
    });
}

// ── Failed sign in backoff ────────────────────────────────────────────────────

/// Consecutive failed sign in attempts for one account.
pub struct FailureEntry {
    pub consecutive: u32,
    /// When the account may next attempt. In the past means it may attempt now.
    pub blocked_until: std::time::Instant,
}

pub type LoginFailures = DashMap<String, FailureEntry>;

/// Failures allowed before any delay is applied.
const FREE_ATTEMPTS: u32 = 5;
/// Delay applied on the first failure past the free allowance.
const BASE_DELAY_SECS: u64 = 30;
/// Longest delay, reached after ten consecutive failures.
const MAX_DELAY_SECS: u64 = 900;

/// Delay owed after `consecutive` failures. Nothing for the first five, then
/// thirty seconds doubling per further failure, capped at fifteen minutes.
///
///   1 to 5 -> 0s, 6 -> 30s, 7 -> 60s, 8 -> 120s, 9 -> 240s, 10 -> 480s,
///   11 and beyond -> 900s
pub fn backoff_secs(consecutive: u32) -> u64 {
    if consecutive <= FREE_ATTEMPTS {
        return 0;
    }
    let doublings = consecutive - FREE_ATTEMPTS - 1;
    BASE_DELAY_SECS
        .checked_shl(doublings)
        .map_or(MAX_DELAY_SECS, |d| d.min(MAX_DELAY_SECS))
}

/// Seconds the account must wait, or None if it may attempt now.
///
/// Called before the password is hashed. Verifying a bcrypt hash at the default
/// cost is deliberately expensive, so an unthrottled login endpoint is both a
/// guessing oracle and a way to saturate the blocking pool; refusing here means
/// a blocked account costs a map lookup instead.
pub fn attempt_blocked_for(failures: &Arc<LoginFailures>, key: &str) -> Option<u64> {
    let entry = failures.get(key)?;
    let now = std::time::Instant::now();
    if entry.blocked_until > now {
        Some((entry.blocked_until - now).as_secs().max(1))
    } else {
        None
    }
}

/// Records a failed attempt and returns the delay now owed.
pub fn record_failure(failures: &Arc<LoginFailures>, key: &str) -> u64 {
    let mut entry = failures
        .entry(key.to_string())
        .or_insert_with(|| FailureEntry {
            consecutive: 0,
            blocked_until: std::time::Instant::now(),
        });
    entry.consecutive = entry.consecutive.saturating_add(1);
    let delay = backoff_secs(entry.consecutive);
    entry.blocked_until = std::time::Instant::now() + std::time::Duration::from_secs(delay);
    delay
}

/// Clears the record after a successful sign in.
pub fn clear_failures(failures: &Arc<LoginFailures>, key: &str) {
    failures.remove(key);
}

/// The response a blocked account receives.
pub fn too_many_attempts_response(retry_after_secs: u64) -> Response {
    (
        StatusCode::TOO_MANY_REQUESTS,
        Json(serde_json::json!({
            "error": "too_many_attempts",
            "retry_after_seconds": retry_after_secs,
            "message": "Too many failed sign in attempts. Try again later.",
        })),
    )
        .into_response()
}

// ── Token version cache ───────────────────────────────────────────────────────

/// Current token version for an account, cached beside the plan.
///
/// The cache is what makes the per request check cheap, and it is also the
/// hazard: any path that invalidates sessions must call `clear_user_cache`, or
/// this keeps handing back the pre change value and the invalidation does
/// nothing. `update_user_plan` already clears it; `change_password` and
/// `reset_password` must too.
pub async fn resolve_token_version(
    counter: &Arc<UsageCounter>,
    db: &Arc<Mutex<Store>>,
    email: &str,
) -> i64 {
    if let Some(entry) = counter.get(email) {
        return entry.token_version;
    }
    let (version, plan) = {
        let guard = db.lock().await;
        (
            guard.get_token_version(email).unwrap_or(0),
            guard
                .get_user_plan(email)
                .unwrap_or_else(|_| "free".to_string()),
        )
    };
    let now_ts = chrono::Utc::now().timestamp();
    let p_start = current_period_start(&plan, now_ts);
    counter.entry(email.to_string()).or_insert(UsageEntry {
        count: 0,
        period_start: p_start,
        plan,
        token_version: version,
    });
    version
}

/// Drops the cached plan and token version for an account. Call after anything
/// that changes either.
pub fn clear_user_cache(counter: &Arc<UsageCounter>, email: &str) {
    counter.remove(email);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Five attempts are free, then thirty seconds doubling per further failure,
    /// capped at fifteen minutes. Stated here so the curve cannot drift silently.
    #[test]
    fn the_backoff_curve_is_what_it_claims() {
        let expected = [
            (1, 0),
            (2, 0),
            (3, 0),
            (4, 0),
            (5, 0),
            (6, 30),
            (7, 60),
            (8, 120),
            (9, 240),
            (10, 480),
            (11, 900),
            (12, 900),
            (50, 900),
            (u32::MAX, 900),
        ];
        for (consecutive, secs) in expected {
            assert_eq!(
                backoff_secs(consecutive),
                secs,
                "failure {consecutive} should owe {secs}s"
            );
        }
    }

    #[test]
    fn failures_accumulate_and_a_success_clears_them() {
        let failures: Arc<LoginFailures> = Arc::new(DashMap::new());
        let key = "user@example.com";

        // Inside the free allowance nothing blocks.
        for _ in 0..FREE_ATTEMPTS {
            assert_eq!(record_failure(&failures, key), 0);
            assert_eq!(attempt_blocked_for(&failures, key), None);
        }

        // The sixth failure starts the backoff.
        assert_eq!(record_failure(&failures, key), 30);
        let wait = attempt_blocked_for(&failures, key).expect("now blocked");
        assert!(wait > 0 && wait <= 30, "wait was {wait}");

        // A success wipes the record, so a legitimate user is not punished for
        // earlier typos.
        clear_failures(&failures, key);
        assert_eq!(attempt_blocked_for(&failures, key), None);
        assert_eq!(record_failure(&failures, key), 0, "counting starts again");
    }

    #[test]
    fn one_account_backing_off_does_not_block_another() {
        let failures: Arc<LoginFailures> = Arc::new(DashMap::new());
        for _ in 0..FREE_ATTEMPTS + 1 {
            record_failure(&failures, "victim@example.com");
        }
        assert!(attempt_blocked_for(&failures, "victim@example.com").is_some());
        assert_eq!(attempt_blocked_for(&failures, "someone@example.com"), None);
    }
}
