//! Pure-function core of the nightly metrics Lambda. The Lambda handler in
//! `main.rs` glues this to live AWS clients; here we only deal in plain
//! [`FeedbackRow`]s so the math is testable.

use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

/// One feedback row as it lives in DynamoDB. Mirrors the schema produced by
/// `kiro_bot::engine::feedback::DynamoFeedbackWriter`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackRow {
    pub slack_msg_id: String,
    /// "+1" for thumbs-up, "-1" for thumbs-down. Anything else is treated as
    /// noise and skipped.
    pub reaction: String,
    pub ts: DateTime<Utc>,
}

/// Result of [`compute_negative_feedback_rate`]. We hold on to the totals as
/// well so the Lambda can emit per-bucket counters alongside the rate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedbackTally {
    pub up: u64,
    pub down: u64,
}

impl FeedbackTally {
    pub fn total(&self) -> u64 {
        self.up + self.down
    }

    /// `down / (up + down)` — 0.0 when there's no signal.
    pub fn negative_rate(&self) -> f64 {
        if self.total() == 0 {
            0.0
        } else {
            self.down as f64 / self.total() as f64
        }
    }
}

/// Tally rows whose timestamp is within `[since, now]`. Bad reactions are
/// silently skipped — the bot writer only writes "+1" / "-1" but the metric
/// shouldn't crash on garbage left from manual edits.
pub fn tally_in_window(
    rows: &[FeedbackRow],
    since: DateTime<Utc>,
    now: DateTime<Utc>,
) -> FeedbackTally {
    let mut up = 0u64;
    let mut down = 0u64;
    for row in rows {
        if row.ts < since || row.ts > now {
            continue;
        }
        match row.reaction.as_str() {
            "+1" => up += 1,
            "-1" => down += 1,
            _ => {},
        }
    }
    FeedbackTally { up, down }
}

/// Compute negative-feedback rate for the spec's 7-day rolling window.
pub fn compute_negative_feedback_rate(
    rows: &[FeedbackRow],
    now: DateTime<Utc>,
) -> FeedbackTally {
    let since = now - chrono::Duration::days(7);
    tally_in_window(rows, since, now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).single().unwrap()
    }

    fn row(reaction: &str, secs: i64) -> FeedbackRow {
        FeedbackRow {
            slack_msg_id: format!("msg-{secs}"),
            reaction: reaction.into(),
            ts: ts(secs),
        }
    }

    #[test]
    fn empty_input_is_zero_rate() {
        let tally = compute_negative_feedback_rate(&[], ts(1_700_000_000));
        assert_eq!(tally.total(), 0);
        assert_eq!(tally.negative_rate(), 0.0);
    }

    #[test]
    fn computes_rate_within_window() {
        let now = ts(1_700_000_000);
        let one_day = 24 * 3600;
        // 4 thumbs-up, 1 thumbs-down within window → 20% negative.
        let rows = vec![
            row("+1", 1_700_000_000 - 1 * one_day),
            row("+1", 1_700_000_000 - 2 * one_day),
            row("+1", 1_700_000_000 - 3 * one_day),
            row("+1", 1_700_000_000 - 4 * one_day),
            row("-1", 1_700_000_000 - 5 * one_day),
        ];
        let tally = compute_negative_feedback_rate(&rows, now);
        assert_eq!(tally.up, 4);
        assert_eq!(tally.down, 1);
        assert!((tally.negative_rate() - 0.2).abs() < 1e-9);
    }

    #[test]
    fn drops_rows_outside_window() {
        let now = ts(1_700_000_000);
        let one_day = 24 * 3600;
        let rows = vec![
            row("+1", 1_700_000_000 - 6 * one_day),  // inside (just barely)
            row("-1", 1_700_000_000 - 8 * one_day),  // OUTSIDE 7d window
            row("-1", 1_700_000_000 - 30 * one_day), // OUTSIDE
        ];
        let tally = compute_negative_feedback_rate(&rows, now);
        assert_eq!(tally.up, 1);
        assert_eq!(tally.down, 0);
    }

    #[test]
    fn unknown_reactions_are_ignored() {
        let now = ts(1_700_000_000);
        let rows = vec![
            row("+1", 1_700_000_000 - 1),
            row("heart", 1_700_000_000 - 1),
            row("-1", 1_700_000_000 - 1),
        ];
        let tally = compute_negative_feedback_rate(&rows, now);
        assert_eq!(tally.up, 1);
        assert_eq!(tally.down, 1);
    }

    #[test]
    fn rate_is_zero_when_total_is_zero() {
        let t = FeedbackTally { up: 0, down: 0 };
        assert_eq!(t.negative_rate(), 0.0);
    }

    #[test]
    fn rate_is_one_when_only_negative() {
        let now = ts(1_700_000_000);
        let rows = vec![row("-1", 1_700_000_000 - 1)];
        let tally = compute_negative_feedback_rate(&rows, now);
        assert!((tally.negative_rate() - 1.0).abs() < 1e-9);
    }
}
