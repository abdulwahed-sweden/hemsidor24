//! The studio's cell.
//!
//! One cell, one key, one chain. The studio signs what it did; nobody else
//! signs anything. What that is and is not worth is set out in the crate docs
//! and in the README — read those before showing a receipt to anyone.

use hemsidor24_core::Package;
use sijill_cell::{Cell, CellError};
use sijill_core::SignedClaim;
use sijill_dialect::Dialect;
use thiserror::Error;

use crate::claim::{Event, HandoverClaim};

/// Something that went wrong issuing or reading a claim.
#[derive(Debug, Error)]
pub enum HandoverError {
    /// The cell could not be created, opened, written to or read.
    #[error("the handover cell could not be used")]
    Cell(#[from] CellError),
    /// The claim would not encode — a field over its limit, caught before
    /// anything was signed.
    #[error("the claim could not be encoded")]
    Encode(#[from] sijill_dialect::DialectError),
}

/// The studio, as a Sijill cell.
///
/// Wraps [`Cell`] rather than reimplementing any of it: the chain, the
/// canonical encoding, the signatures and the log are all the protocol's.
/// What this adds is the studio's vocabulary.
pub struct Studio {
    cell: Cell,
}

impl Studio {
    /// Create the studio's cell at `dir`, generating its key.
    ///
    /// Refuses to overwrite an existing key — the key *is* the cell, and a
    /// replaced one cannot continue the chain.
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

    /// The underlying cell, for anything this wrapper does not cover.
    pub fn cell(&self) -> &Cell {
        &self.cell
    }

    /// Sign one claim into the studio's chain.
    pub fn issue(&self, claim: &HandoverClaim) -> Result<SignedClaim, HandoverError> {
        Ok(self.cell.issue(claim)?)
    }

    /// Record that a proposal was shown, before publication.
    ///
    /// This is the promise "du ser förslaget innan sidan publiceras" coming
    /// due, and the moment the money-back guarantee starts to matter.
    pub fn offer_delivery(
        &self,
        order_ref: u64,
        site_ref: u64,
    ) -> Result<SignedClaim, HandoverError> {
        self.issue(&HandoverClaim::new(Event::DeliveryOffered, order_ref).for_site(site_ref))
    }

    /// Record that the customer accepted the proposal.
    pub fn accept_delivery(
        &self,
        order_ref: u64,
        site_ref: u64,
    ) -> Result<SignedClaim, HandoverError> {
        self.issue(&HandoverClaim::new(Event::DeliveryAccepted, order_ref).for_site(site_ref))
    }

    /// Record that a revision was spent. Revision 1 is the included one.
    pub fn use_revision(
        &self,
        order_ref: u64,
        site_ref: u64,
        revision: u32,
    ) -> Result<SignedClaim, HandoverError> {
        self.issue(
            &HandoverClaim::new(Event::RevisionUsed, order_ref)
                .for_site(site_ref)
                .revision(revision),
        )
    }

    /// Record a refund under the guarantee.
    ///
    /// Takes the [`Package`] rather than a number so the amount cannot drift
    /// from the price the customer was actually quoted.
    pub fn refund(
        &self,
        order_ref: u64,
        package: Package,
        reason: impl Into<String>,
    ) -> Result<SignedClaim, HandoverError> {
        self.issue(
            &HandoverClaim::new(Event::RefundIssued, order_ref)
                .refunding_ore(u64::from(package.price_ore_inc_vat()))
                .with_note(reason),
        )
    }

    /// Record that the domain, the hosting and the source passed to the
    /// customer — the promise that gets disputed most.
    pub fn transfer_ownership(
        &self,
        order_ref: u64,
        site_ref: u64,
        domain: impl Into<String>,
        repo_url: impl Into<String>,
    ) -> Result<SignedClaim, HandoverError> {
        self.issue(
            &HandoverClaim::new(Event::OwnershipTransferred, order_ref)
                .for_site(site_ref)
                .transferring(domain, repo_url),
        )
    }

    /// Every handover claim this cell has signed, oldest first.
    ///
    /// Claims in other dialects are skipped: the chain may hold checkpoints and
    /// endorsements this crate has no opinion about.
    pub fn history(&self) -> Result<Vec<(u64, SignedClaim, HandoverClaim)>, HandoverError> {
        let id = self.cell.id();
        let mut out = Vec::new();
        for (seq, signed) in self.cell.claims(&id)? {
            if let Ok(body) = HandoverClaim::from_claim(&signed.claim) {
                out.push((seq, signed, body));
            }
        }
        out.sort_by_key(|(seq, _, _)| *seq);
        Ok(out)
    }

    /// The handover claims concerning one order.
    pub fn history_for_order(
        &self,
        order_ref: u64,
    ) -> Result<Vec<(u64, SignedClaim, HandoverClaim)>, HandoverError> {
        Ok(self
            .history()?
            .into_iter()
            .filter(|(_, _, body)| body.order_ref == order_ref)
            .collect())
    }
}
