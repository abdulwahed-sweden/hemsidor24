//! Signed handover of control, for Hemsidor24.
//!
//! # What is signed, and what is not
//!
//! The public page makes four promises. Only one of them is a question of
//! *fact* that an outsider could later have to settle:
//!
//! | Promise | Where it is recorded | Why |
//! |---|---|---|
//! | You see a proposal before publication | Postgres, `deliveries.offered_at` | A commercial milestone. Nothing changes hands. |
//! | You pay only once you have seen it | Postgres, `deliveries.accepted_at` | Acceptance of an offer, not a transfer. |
//! | One revision is included | Postgres, `sites.revisions_used` | Scope metering under this studio's fixed price. |
//! | Money back before publication | Postgres, `deliveries.refunded_at` | Commercial settlement. |
//! | **Domain, hosting and source are yours** | **Signed handoff claims** | **Control actually leaves the studio, toward a party who can later dispute it.** |
//!
//! The first four already live in Postgres under the back office's audit
//! trail, which records who changed what and when. Signing them as well would
//! duplicate that and, worse, would dress a one-sided assertion up as proof:
//! a studio-signed "the customer accepted" is exactly the claim a customer
//! would contest, and the signature adds nothing against them.
//!
//! The fifth is different. A studio giving up control of an artefact is a
//! custody event with a real counterparty, and the studio's own database is the
//! weakest possible evidence for it precisely because the studio controls it.
//!
//! # What a signed claim asserts, and what it does not
//!
//! A `Released` claim here says: **the studio asserts it gave up control of
//! this artefact.** That is the whole of it.
//!
//! It is not the customer's acknowledgement — there is no customer cell, so
//! there is no `Received` — and it is not evidence that legal title or
//! copyright moved. Whether the customer ends up owning anything is settled by
//! the contract and by the registrar's, host's or forge's own records, none of
//! which this crate speaks for. The receipt is worded to keep that line
//! visible.
//!
//! # No dialect of its own
//!
//! This crate defines **no claim schema**. Control of a thing passing from one
//! party to another is what [`sijill_dialect_handoff`] is for, and a private
//! schema for it would be one company's word for something the domain already
//! has a word for. Dialects belong to a domain, not to a company.
//!
//! Each promised artefact — domain, hosting account, repository — becomes its
//! own `Released` claim naming the customer as counterparty, because the three
//! move separately. A `Discharged` claim closes the studio's own record for an
//! artefact — upstream that means only "the item leaves this chain of
//! accountability", never a discharge of liability — and names the `Released`
//! it answers.
//!
//! # What a receipt is worth
//!
//! **Single-sided provenance, not a mutual exchange.** Sijill is built for
//! participants who each keep and sign their own records and countersign what
//! they received. The studio runs a cell; the customer does not. Every claim
//! is the studio's own account, signed by the studio's own key.
//!
//! It establishes that the studio's key signed exactly this, that it has not
//! changed since, and that entries were not reordered or removed from the
//! middle. It does **not** establish that any of it is true, that the customer
//! agrees, when anything happened, or that nothing was cut from the end of the
//! chain — see `LIMITATIONS.md` in the protocol.
//!
//! Making it mutual needs the customer to hold a key and issue their own
//! `Received` against ours. The dialect is already shaped for that: the day a
//! customer runs a cell, their acceptance becomes a real second signature and
//! nothing here has to change. Asking a small-business owner in Sweden to
//! manage a signing key today is a cost the product's promise does not carry.
//!
//! # Isolation
//!
//! Compiled out unless the `handover` feature is on, and nothing else in this
//! workspace depends on it.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

#[cfg(feature = "handover")]
pub mod asset;
#[cfg(feature = "handover")]
pub mod receipt;
#[cfg(feature = "handover")]
pub mod studio;

#[cfg(feature = "handover")]
pub use asset::{Artefact, Asset, customer_ref, order_ref};
#[cfg(feature = "handover")]
pub use receipt::JournalEntry;
#[cfg(feature = "handover")]
pub use studio::{Entry, HandoverError, Studio};

// Re-exported because this crate's signatures name them: a caller cannot hold
// what `Studio::release_custody` returns without being able to name its type.
#[cfg(feature = "handover")]
pub use sijill_cell::{CellError, format_timestamp};
#[cfg(feature = "handover")]
pub use sijill_core::{Claim, ClaimId, SignedClaim};
#[cfg(feature = "handover")]
pub use sijill_dialect_handoff::{Event, Handoff};
