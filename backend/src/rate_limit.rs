use std::sync::Arc;

use axum::{Json, http::StatusCode, response::{IntoResponse, Response}};
use dashmap::DashMap;
use tokio::sync::Mutex;
use tracing::warn;

use crate::db::Db;

pub struct UsageEntry {
    pub count: u64,
    pub period_start: i64,
    pub plan: String,
}

pub type UsageCounter = DashMap<String, UsageEntry>;

pub fn plan_limit(plan: &str) -> Option<u64> {
    match plan {
        "free" | "starter" => Some(100),
        "developer"        => Some(10_000),
        "pro"              => Some(100_000),
        "business"         => Some(1_000_000),
        "enterprise"       => None,
        _                  => Some(100),
    }
}

fn is_daily(plan: &str) -> bool {
    matches!(plan, "free" | "starter")
}

pub fn current_period_start(plan: &str, now: i64) -> i64 {
    if is_daily(plan) {
        now - (now % 86_400)
    } else {
        use chrono::{Datelike, TimeZone, Utc};
        Utc.timestamp_opt(now, 0)
            .single()
            .and_then(|dt| {
                Utc.with_ymd_and_hms(dt.year(), dt.month(), 1, 0, 0, 0).single()
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
    db: &Arc<Mutex<Db>>,
    email: &str,
) -> Result<(), Response> {
    let now_ts = chrono::Utc::now().timestamp();

    // Hot path: entry present and period still valid — check + increment under shard lock.
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
        // Period expired — fall through; shard lock released here.
    }

    // Cold path: no entry yet or period rolled — fetch plan from DB.
    let plan = {
        let guard = db.lock().await;
        guard.get_user_plan(email).unwrap_or_else(|_| "starter".to_string())
    };
    let p_start = current_period_start(&plan, now_ts);
    let p_end   = period_end(&plan, p_start);

    // Insert or get existing entry under shard lock (no await below this point).
    let mut entry = counter
        .entry(email.to_string())
        .or_insert_with(|| UsageEntry { count: 0, period_start: p_start, plan: plan.clone() });

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
pub fn spawn_flush_task(counter: Arc<UsageCounter>, db: Arc<Mutex<Db>>) {
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
            if snapshots.is_empty() {
                continue;
            }
            let guard = db.lock().await;
            for (email, count, period_start, plan) in snapshots {
                let p_end = period_end(&plan, period_start);
                if let Err(e) = guard.upsert_usage_record(&email, count as i64, period_start, p_end)
                {
                    warn!("usage flush error for {email}: {e}");
                }
            }
        }
    });
}
