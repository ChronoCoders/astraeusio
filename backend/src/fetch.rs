//! What a poll came back with, kept separate from the rows themselves.
//!
//! A poller can come back with nothing in three different ways, and they mean
//! three different things:
//!
//! - upstream said explicitly that nothing has changed, which is normal
//! - upstream answered with a payload holding no entries at all
//! - upstream sent entries and every one of them failed to parse
//!
//! The third is a broken feed contract and the first is a healthy Tuesday, but
//! logged as a bare count they are the same line. That is how the IMF feed sat
//! dead for forty days: the poller logged a number every minute and nothing in
//! the log said which number meant trouble.
//!
//! Feeds that parse strictly, where serde rejects the whole payload rather than
//! dropping rows, cannot reach the third case at all; for those the row count
//! really is the entire truth and [`PollOutcome::strict`] says so. Feeds that
//! parse row by row with `filter_map` can, so they report both counts.

/// How a fetch ended, beyond the rows it produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollOutcome {
    /// Upstream told us there is nothing new since the last successful fetch,
    /// so it deliberately sent no rows. Normal operation, not missing data.
    NoChange,
    /// Upstream answered normally with a payload containing no entries.
    EmptyPayload,
    /// Upstream sent `received` entries and `kept` of them survived parsing.
    Parsed { received: usize, kept: usize },
}

impl PollOutcome {
    /// For feeds deserialized strictly into a typed vector. Serde fails the
    /// whole payload on a malformed entry, so a row can never be dropped
    /// silently and `received` always equals `kept`.
    pub fn strict(len: usize) -> Self {
        if len == 0 {
            PollOutcome::EmptyPayload
        } else {
            PollOutcome::Parsed { received: len, kept: len }
        }
    }

    /// For feeds parsed entry by entry, where entries can be skipped.
    pub fn lossy(received: usize, kept: usize) -> Self {
        if received == 0 {
            PollOutcome::EmptyPayload
        } else {
            PollOutcome::Parsed { received, kept }
        }
    }
}

/// Rows plus the outcome that produced them.
#[derive(Debug)]
pub struct Fetched<T> {
    pub items: Vec<T>,
    pub outcome: PollOutcome,
}

impl<T> Fetched<T> {
    /// Upstream reported no change, so there is nothing to write.
    pub fn no_change() -> Self {
        Fetched { items: Vec::new(), outcome: PollOutcome::NoChange }
    }

    /// `received` is the number of entries upstream sent, before parsing.
    pub fn lossy(items: Vec<T>, received: usize) -> Self {
        let outcome = PollOutcome::lossy(received, items.len());
        Fetched { items, outcome }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_zero_is_an_empty_payload_not_a_parse_failure() {
        // A strict feed cannot drop rows, so zero rows can only mean the
        // payload was empty. Reporting it as Parsed{0,0} would accuse the
        // parser of losing data it never received.
        assert_eq!(PollOutcome::strict(0), PollOutcome::EmptyPayload);
        assert_eq!(
            PollOutcome::strict(3),
            PollOutcome::Parsed { received: 3, kept: 3 }
        );
    }

    #[test]
    fn lossy_separates_an_empty_payload_from_a_total_parse_failure() {
        // Nothing sent.
        assert_eq!(PollOutcome::lossy(0, 0), PollOutcome::EmptyPayload);
        // Rows sent, none survived. This is the case that must never look like
        // the one above, because it means the feed shape changed under us.
        assert_eq!(
            PollOutcome::lossy(120, 0),
            PollOutcome::Parsed { received: 120, kept: 0 }
        );
        // Partial loss.
        assert_eq!(
            PollOutcome::lossy(120, 118),
            PollOutcome::Parsed { received: 120, kept: 118 }
        );
    }

    #[test]
    fn no_change_carries_no_rows() {
        let f: Fetched<u8> = Fetched::no_change();
        assert!(f.items.is_empty());
        assert_eq!(f.outcome, PollOutcome::NoChange);
    }
}
