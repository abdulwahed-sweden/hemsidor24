//! The studio's cell, speaking the first-party handoff dialect.
//!
//! One cell, one key, one chain. This crate defines **no dialect of its own**:
//! ownership passing from the studio to a customer is a custody event, and
//! `sijill-dialect-handoff` already says what a custody event is. A private
//! schema for it would be this company's word for something the domain already
//! has a word for.
//!
//! What it does own is the decision about *which* business facts are worth a
//! signed claim at all — see the crate docs.

use sijill_cell::{Cell, CellError};
use sijill_core::{ClaimId, SignedClaim};
use sijill_dialect::Dialect;
use sijill_dialect_handoff::{Event, Handoff};
use thiserror::Error;

use crate::asset::{Asset, Transfer, customer_ref};

/// Something that went wrong issuing or reading a claim.
#[derive(Debug, Error)]
pub enum HandoverError {
    /// The cell could not be created, opened, written to or read.
    #[error("the handover cell could not be used")]
    Cell(#[from] CellError),
    /// The claim would not encode — a reference over its bound, caught before
    /// anything was signed.
    #[error("the handoff claim could not be built")]
    Dialect(#[from] sijill_dialect::DialectError),
    /// Nothing was handed over, so there is nothing to sign.
    #[error("a transfer must name at least one asset")]
    NothingToTransfer,
}

/// One signed handoff claim, with the body already decoded.
pub type Entry = (u64, SignedClaim, Handoff);

/// The studio, as a Sijill cell.
///
/// Wraps [`Cell`] rather than reimplementing any of it: the chain, the
/// canonical encoding, the signatures and the log are all the protocol's, and
/// the claim bodies are the handoff dialect's.
pub struct Studio {
    cell: Cell,
}

impl Studio {
    /// Create the studio's cell at `dir`, generating its key.
    pub fn create(dir: impl AsRef<std::path::Path>) -> Result<Self, HandoverError> {
        Ok(Studio {
            cell: Cell::create(dir)?,
        })
    }

    /// Open the studio's existing cell.
    pub fn open(dir: impl AsRef<std::path::Path>) -> Result<Self, HandoverError> {
        Ok(Studio {
            cell: Cell::open(dir)?,
        })
    }

    /// The underlying cell.
    pub fn cell(&self) -> &Cell {
        &self.cell
    }

    /// Sign that the named artefacts passed to the customer.
    ///
    /// One `Released` claim per artefact, because they move separately and a
    /// reader asking "was the domain actually put in their name" should not
    /// have to unpack a bundle to find out. Each names the customer as
    /// counterparty, which the dialect requires for `Released`.
    ///
    /// This is the only business fact this crate signs. Everything else the
    /// studio does — showing a proposal, recording an acceptance, spending a
    /// revision, issuing a refund — is commercial state that already lives in
    /// Postgres under the back office's audit trail, and is carried into the
    /// receipt from there.
    pub fn transfer_ownership(
        &self,
        customer_id: i64,
        transfers: &[Transfer],
    ) -> Result<Vec<SignedClaim>, HandoverError> {
        if transfers.is_empty() {
            return Err(HandoverError::NothingToTransfer);
        }

        let counterparty = customer_ref(customer_id);
        let mut issued = Vec::with_capacity(transfers.len());

        for transfer in transfers {
            let body = Handoff::new(transfer.item_ref(), Event::Released)?
                .with_counterparty(counterparty.clone())?;
            issued.push(self.cell.issue(&body)?);
        }

        Ok(issued)
    }

    /// Sign that the studio's custody of an artefact has ended.
    ///
    /// `Discharged` is the dialect's answer to a chain that simply stops: it
    /// says the item left this chain of accountability deliberately, rather
    /// than the record being withheld. `acknowledges` points at the `Released`
    /// it closes, so the pair can be checked instead of matched by guesswork.
    ///
    /// The dialect refuses a counterparty on `Discharged` — it is about the
    /// item in one party's hands — so none is set.
    pub fn discharge(
        &self,
        transfer: &Transfer,
        released: ClaimId,
    ) -> Result<SignedClaim, HandoverError> {
        let body = Handoff::new(transfer.item_ref(), Event::Discharged)?.acknowledging(released);
        Ok(self.cell.issue(&body)?)
    }

    /// Every handoff claim this cell has signed, oldest first.
    ///
    /// Claims in other dialects are skipped: a chain may hold checkpoints and
    /// endorsements this crate has no opinion about.
    pub fn history(&self) -> Result<Vec<Entry>, HandoverError> {
        let id = self.cell.id();
        let mut out = Vec::new();
        for (seq, signed) in self.cell.claims(&id)? {
            if let Ok(body) = Handoff::from_claim(&signed.claim) {
                out.push((seq, signed, body));
            }
        }
        out.sort_by_key(|(seq, _, _)| *seq);
        Ok(out)
    }

    /// The handoff claims concerning one customer.
    pub fn history_for_customer(&self, customer_id: i64) -> Result<Vec<Entry>, HandoverError> {
        let wanted = customer_ref(customer_id);
        Ok(self
            .history()?
            .into_iter()
            .filter(|(_, _, body)| {
                body.counterparty.as_deref() == Some(wanted.as_str())
                    || body.event == Event::Discharged
            })
            .collect())
    }
}

/// Which promised asset an item reference belongs to, if any.
///
/// The prefix is the studio's own convention, so reading it back is this
/// crate's job rather than the dialect's.
pub fn asset_of(item: &str) -> Option<Asset> {
    Asset::ALL
        .into_iter()
        .find(|a| item.starts_with(&format!("{}:", a.prefix())))
}

/// The artefact identifier inside an item reference.
pub fn identifier_of(item: &str) -> &str {
    item.split_once(':').map_or(item, |(_, rest)| rest)
}
