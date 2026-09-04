//! Where an order is in its life, and which moves are legal from there.

use thiserror::Error;

/// The status of an order in the back office.
///
/// The happy path is linear:
///
/// ```text
/// Ny → UtkastSkickat → Godkand → Publicerad
/// ```
///
/// [`OrderStatus::Avbruten`] can be reached from any point before publication;
/// once a site is live, cancelling is a refund, which is a different event.
/// Both `Publicerad` and `Avbruten` are terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OrderStatus {
    /// Order received, nobody has looked at it yet.
    Ny,
    /// A draft has been sent to the customer for review.
    UtkastSkickat,
    /// The customer approved the draft.
    Godkand,
    /// The site is live.
    Publicerad,
    /// The order was cancelled before publication.
    Avbruten,
}

impl OrderStatus {
    /// Every status, in the order of the happy path, cancellation last.
    pub const ALL: [OrderStatus; 5] = [
        OrderStatus::Ny,
        OrderStatus::UtkastSkickat,
        OrderStatus::Godkand,
        OrderStatus::Publicerad,
        OrderStatus::Avbruten,
    ];

    /// The status of a freshly received order.
    pub const INITIAL: OrderStatus = OrderStatus::Ny;

    /// Stable identifier written to the database.
    pub const fn slug(self) -> &'static str {
        match self {
            OrderStatus::Ny => "ny",
            OrderStatus::UtkastSkickat => "utkast_skickat",
            OrderStatus::Godkand => "godkand",
            OrderStatus::Publicerad => "publicerad",
            OrderStatus::Avbruten => "avbruten",
        }
    }

    /// Label as shown in the back office.
    pub const fn label_sv(self) -> &'static str {
        match self {
            OrderStatus::Ny => "Ny",
            OrderStatus::UtkastSkickat => "Utkast skickat",
            OrderStatus::Godkand => "Godkänd",
            OrderStatus::Publicerad => "Publicerad",
            OrderStatus::Avbruten => "Avbruten",
        }
    }

    /// Whether the order is finished, one way or the other.
    pub const fn is_terminal(self) -> bool {
        matches!(self, OrderStatus::Publicerad | OrderStatus::Avbruten)
    }

    /// The statuses this one may move to.
    pub const fn next_allowed(self) -> &'static [OrderStatus] {
        match self {
            OrderStatus::Ny => &[OrderStatus::UtkastSkickat, OrderStatus::Avbruten],
            OrderStatus::UtkastSkickat => &[OrderStatus::Godkand, OrderStatus::Avbruten],
            OrderStatus::Godkand => &[OrderStatus::Publicerad, OrderStatus::Avbruten],
            OrderStatus::Publicerad | OrderStatus::Avbruten => &[],
        }
    }

    /// Whether moving to `next` is legal.
    pub fn can_transition_to(self, next: OrderStatus) -> bool {
        self.next_allowed().contains(&next)
    }

    /// Move to `next`, or explain why not.
    ///
    /// The back office in phase 4 records every accepted transition; this is
    /// the gate it goes through.
    pub fn transition_to(self, next: OrderStatus) -> Result<OrderStatus, TransitionError> {
        if self.can_transition_to(next) {
            Ok(next)
        } else {
            Err(TransitionError {
                from: self,
                to: next,
            })
        }
    }
}

/// An attempt to move an order somewhere it cannot go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("cannot move an order from {} to {}", from.slug(), to.slug())]
pub struct TransitionError {
    /// Status the order was in.
    pub from: OrderStatus,
    /// Status that was asked for.
    pub to: OrderStatus,
}

#[cfg(test)]
mod tests {
    use super::OrderStatus::*;
    use super::*;

    #[test]
    fn a_new_order_starts_as_ny() {
        assert_eq!(OrderStatus::INITIAL, Ny);
    }

    #[test]
    fn the_happy_path_walks_end_to_end() {
        let mut s = OrderStatus::INITIAL;
        for next in [UtkastSkickat, Godkand, Publicerad] {
            s = s
                .transition_to(next)
                .expect("happy path step should be legal");
        }
        assert_eq!(s, Publicerad);
    }

    #[test]
    fn cancellation_is_reachable_before_publication() {
        for s in [Ny, UtkastSkickat, Godkand] {
            assert!(s.can_transition_to(Avbruten), "{s:?} should be cancellable");
        }
    }

    #[test]
    fn terminal_statuses_go_nowhere() {
        for s in [Publicerad, Avbruten] {
            assert!(s.is_terminal());
            assert!(s.next_allowed().is_empty());
            for target in OrderStatus::ALL {
                assert!(
                    !s.can_transition_to(target),
                    "{s:?} should not move to {target:?}"
                );
            }
        }
    }

    #[test]
    fn steps_cannot_be_skipped() {
        assert!(!Ny.can_transition_to(Godkand));
        assert!(!Ny.can_transition_to(Publicerad));
        assert!(!UtkastSkickat.can_transition_to(Publicerad));
    }

    #[test]
    fn an_order_cannot_go_backwards() {
        assert!(!UtkastSkickat.can_transition_to(Ny));
        assert!(!Godkand.can_transition_to(UtkastSkickat));
        assert!(!Publicerad.can_transition_to(Godkand));
    }

    #[test]
    fn no_status_transitions_to_itself() {
        for s in OrderStatus::ALL {
            assert!(
                !s.can_transition_to(s),
                "{s:?} should not transition to itself"
            );
        }
    }

    #[test]
    fn illegal_transition_reports_both_ends() {
        let err = Ny
            .transition_to(Publicerad)
            .expect_err("skipping steps is illegal");
        assert_eq!(err.from, Ny);
        assert_eq!(err.to, Publicerad);
        assert_eq!(
            err.to_string(),
            "cannot move an order from ny to publicerad"
        );
    }

    #[test]
    fn slugs_are_unique() {
        let mut slugs: Vec<_> = OrderStatus::ALL.iter().map(|s| s.slug()).collect();
        slugs.sort_unstable();
        let count = slugs.len();
        slugs.dedup();
        assert_eq!(slugs.len(), count);
    }

    #[test]
    fn every_status_is_reachable_from_the_initial_one() {
        // Breadth-first walk from Ny; nothing should be stranded.
        let mut seen = vec![OrderStatus::INITIAL];
        let mut queue = vec![OrderStatus::INITIAL];
        while let Some(s) = queue.pop() {
            for &next in s.next_allowed() {
                if !seen.contains(&next) {
                    seen.push(next);
                    queue.push(next);
                }
            }
        }
        assert_eq!(seen.len(), OrderStatus::ALL.len());
    }
}
