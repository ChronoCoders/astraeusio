use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{db::Store, rate_limit::UsageCounter};

// ── Hierarchy ─────────────────────────────────────────────────────────────────
// free/starter < developer < pro < business < enterprise

fn rank(plan: &str) -> u8 {
    match plan {
        "free" | "starter" => 0,
        "developer" => 1,
        "pro" => 2,
        "business" => 3,
        "enterprise" => 4,
        _ => 0,
    }
}

pub fn satisfies(user_plan: &str, required: &str) -> bool {
    rank(user_plan) >= rank(required)
}

/// Resolve the user's current plan from the in-memory counter (fast path)
/// or fall back to a database read (cold path).
pub async fn resolve(counter: &Arc<UsageCounter>, db: &Arc<Mutex<Store>>, email: &str) -> String {
    if let Some(entry) = counter.get(email) {
        return entry.plan.clone();
    }
    db.lock()
        .await
        .get_user_plan(email)
        .unwrap_or_else(|_| "starter".to_string())
}
