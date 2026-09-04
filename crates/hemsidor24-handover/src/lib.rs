//! Sijill handover claims for Hemsidor24.
//!
//! When a site is delivered the studio promises four things out loud: that the
//! customer owns the domain, the hosting and the source; that one revision is
//! included; and that they get their money back if they are not satisfied
//! before publication. Those are exactly the things that get disputed a year
//! later, when everyone remembers it differently.
//!
//! This crate writes them down as signed, append-only claims using the Sijill
//! protocol, and renders a receipt from them.
//!
//! # What this is worth, and what it is not
//!
//! **This is single-sided provenance, not a mutual exchange.** Sijill is built
//! for independent participants who each keep and sign their own records, and
//! who countersign what they received from the other. That is not what happens
//! here. The studio runs a cell; the customer does not. Every claim in the log
//! is the studio's own account of what the studio did, signed by the studio's
//! own key.
//!
//! So the receipt establishes:
//!
//! - the studio's key signed exactly this text, and
//! - the text has not been altered since, and
//! - the entries have not been reordered or removed from the middle.
//!
//! It does **not** establish:
//!
//! - that any of it is true. A cell can sign something false as easily as
//!   something true; Sijill's own documentation is emphatic about this.
//! - that the customer agrees. They signed nothing and were never asked to.
//!   "DeliveryAccepted" means the studio recorded an acceptance, not that the
//!   customer attested to one.
//! - when anything happened. Timestamps are asserted by the signer.
//! - that nothing was left off the end. Truncating the tail of a chain is not
//!   detectable from the chain alone — see `LIMITATIONS.md` in the protocol.
//!
//! Against a customer who says "I never approved that", this is the studio's
//! contemporaneous, tamper-evident note to itself. That is genuinely better
//! than a mutable row in a database the studio also controls, and it is
//! genuinely weaker than a countersigned record. Anyone shown a receipt should
//! be told which of those they are looking at, which is why the caveats are
//! printed on the receipt itself rather than kept in a README.
//!
//! Making it mutual needs the customer to hold a key and issue their own
//! acceptance claim against ours. That is a product decision — asking a
//! small-business owner in Sweden to manage a signing key is a real cost, and
//! the promise of this product is that nothing about it is annoying.
//!
//! # Isolation
//!
//! Compiled out unless the `handover` feature is on, and nothing else in this
//! workspace may depend on it. It is an experiment that may be deleted.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

#[cfg(feature = "handover")]
pub mod claim;
#[cfg(feature = "handover")]
pub mod receipt;
#[cfg(feature = "handover")]
pub mod studio;

#[cfg(feature = "handover")]
pub use claim::{Event, HandoverClaim};
#[cfg(feature = "handover")]
pub use studio::{HandoverError, Studio};

// Re-exported because this crate's own signatures name them: a caller cannot
// hold what `Studio::issue` returns without being able to name its type.
#[cfg(feature = "handover")]
pub use sijill_cell::{CellError, format_timestamp};
#[cfg(feature = "handover")]
pub use sijill_core::{Body, Claim, ClaimId, SignedClaim};
#[cfg(feature = "handover")]
pub use sijill_dialect::{Dialect, DialectError};
