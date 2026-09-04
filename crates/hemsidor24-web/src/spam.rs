//! Quiet spam protection.
//!
//! The product promise is that nothing about this is annoying, so there is no
//! captcha and nothing for a human to solve. Three cheap checks instead, in the
//! order they cost the customer least:
//!
//! 1. a honeypot field, invisible to people and irresistible to naive bots,
//! 2. a minimum time between rendering the form and submitting it,
//! 3. a per-IP rate limit, applied by `tower_governor` in [`crate::routes`].
//!
//! A caught submission is answered with the same confirmation a real one gets.
//! Telling a bot why it failed only helps it try again.

use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds a human needs, at minimum, to fill this form in.
///
/// Six fields including a free-text list of services. Three seconds is well
/// under what anyone typing honestly will take, and well over what a script
/// posting straight to the endpoint will wait.
pub const MIN_SECONDS_ON_PAGE: u64 = 3;

/// Why a submission was treated as spam.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpamVerdict {
    /// Looks like a person.
    Clean,
    /// The invisible field was filled in.
    HoneypotFilled,
    /// Submitted sooner than a human could have typed it.
    TooFast {
        /// How long they actually took.
        seconds: u64,
    },
}

impl SpamVerdict {
    /// Whether the submission should be quietly dropped.
    pub fn is_spam(self) -> bool {
        !matches!(self, SpamVerdict::Clean)
    }
}

/// Seconds since the Unix epoch, or 0 if the clock is before 1970.
pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Judge a submission.
///
/// `opened_at` is the value that was rendered into the form. It is not signed,
/// so a determined bot can forge it — the honeypot and the rate limit are what
/// catch that one. Signing it would need a MAC and a secret, which is a
/// worthwhile change the day this stops being enough.
pub fn check(honeypot: &str, opened_at: &str) -> SpamVerdict {
    if !honeypot.trim().is_empty() {
        return SpamVerdict::HoneypotFilled;
    }

    // An unparseable or missing timestamp is treated as fine: a real customer
    // with a stripped form should never be turned away over a hidden field.
    let Ok(opened) = opened_at.trim().parse::<u64>() else {
        return SpamVerdict::Clean;
    };

    let elapsed = now_unix().saturating_sub(opened);
    if elapsed < MIN_SECONDS_ON_PAGE {
        return SpamVerdict::TooFast { seconds: elapsed };
    }

    SpamVerdict::Clean
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_honeypot_and_a_patient_human_pass() {
        let opened = now_unix() - 30;
        assert_eq!(check("", &opened.to_string()), SpamVerdict::Clean);
    }

    #[test]
    fn a_filled_honeypot_is_spam() {
        let opened = now_unix() - 30;
        assert_eq!(
            check("http://spam.example", &opened.to_string()),
            SpamVerdict::HoneypotFilled
        );
        assert!(check("x", &opened.to_string()).is_spam());
    }

    #[test]
    fn whitespace_in_the_honeypot_is_not_a_fill() {
        let opened = now_unix() - 30;
        assert_eq!(check("   ", &opened.to_string()), SpamVerdict::Clean);
    }

    #[test]
    fn an_instant_submission_is_spam() {
        let verdict = check("", &now_unix().to_string());
        assert!(verdict.is_spam(), "{verdict:?}");
        assert!(matches!(verdict, SpamVerdict::TooFast { .. }));
    }

    #[test]
    fn the_honeypot_wins_over_the_timer() {
        assert_eq!(
            check("x", &now_unix().to_string()),
            SpamVerdict::HoneypotFilled
        );
    }

    #[test]
    fn a_missing_or_broken_timestamp_never_blocks_a_customer() {
        for raw in ["", "not-a-number", "-5"] {
            assert_eq!(check("", raw), SpamVerdict::Clean, "{raw:?}");
        }
    }

    #[test]
    fn a_timestamp_from_the_future_is_treated_as_too_fast() {
        // The value is minted server-side, so a future one was tampered with.
        let future = now_unix() + 600;
        assert_eq!(
            check("", &future.to_string()),
            SpamVerdict::TooFast { seconds: 0 }
        );
    }
}
