//! Signing the handoff when a site is published.
//!
//! Compiled out entirely unless the `handover` feature is on, so the ordinary
//! back office carries no protocol code. With it on, publishing signs one
//! `Released` claim per artefact that actually passed to the customer.
//!
//! # What is signed, and what is not
//!
//! Only artefacts we can name. A domain we hold no name for, a hosting account
//! with no reference, a repository with no URL — none of those get a claim,
//! because a claim naming nothing establishes nothing. The delivery flag for an
//! artefact is set only when its claim was signed, so the record cannot say
//! "hosting transferred" on the strength of an empty column.
//!
//! And the claim says only what it says: the studio asserts it released control
//! of that artefact. There is no customer cell, so nothing here is the
//! customer's acknowledgement, and none of it establishes legal ownership —
//! that is settled by the agreement and by the registrar's, host's and forge's
//! own records.

/// What publishing found to hand over, drawn from the site row.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Artefacts {
    /// The domain, from `domains.name`.
    pub domain: Option<String>,
    /// The hosting account, from `sites.hosting_ref`.
    pub hosting: Option<String>,
    /// The repository, from `sites.repo_url`.
    pub source: Option<String>,
}

impl Artefacts {
    /// Whether there is anything to sign at all.
    pub fn is_empty(&self) -> bool {
        self.domain.is_none() && self.hosting.is_none() && self.source.is_none()
    }
}

/// What was actually signed, so the caller can set exactly those flags.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Signed {
    /// A `Released` claim naming the domain was signed.
    pub domain: bool,
    /// A `Released` claim naming the hosting account was signed.
    pub hosting: bool,
    /// A `Released` claim naming the repository was signed.
    pub source: bool,
}

#[cfg(feature = "handover")]
mod enabled {
    use std::sync::OnceLock;

    use hemsidor24_handover::{Artefact, Asset, Studio};

    use super::{Artefacts, Signed};

    /// Where the studio's cell lives on disk, if signing is configured.
    static CELL_DIR: OnceLock<Option<std::path::PathBuf>> = OnceLock::new();

    /// Where the cell lives, resolved once from `HANDOVER_CELL_DIR`.
    ///
    /// Resolved on first use rather than only at startup. [`init`] exists so a
    /// misconfiguration is reported when the process starts rather than at the
    /// first publication, but forgetting to call it must not silently turn
    /// signing off — that is the kind of ordering trap that is invisible until
    /// the record it was meant to produce is missing.
    fn cell_dir() -> Option<&'static std::path::PathBuf> {
        CELL_DIR
            .get_or_init(|| {
                std::env::var("HANDOVER_CELL_DIR")
                    .ok()
                    .filter(|v| !v.trim().is_empty())
                    .map(std::path::PathBuf::from)
            })
            .as_ref()
    }

    /// Whether a cell is configured at all.
    ///
    /// Exists for tests. Whether a handover gets signed depends on two things
    /// — the feature being on *and* `HANDOVER_CELL_DIR` pointing somewhere —
    /// and a test that checks only the first silently passes or fails on
    /// whatever happens to be in the environment.
    #[cfg(test)]
    pub fn configured() -> bool {
        cell_dir().is_some()
    }

    /// Report at startup whether signing is configured, and open the cell once
    /// so a broken one is found now rather than mid-handover.
    ///
    /// Absent means publishing still works and simply signs nothing, logged
    /// loudly — the same policy as mail. A cell that cannot be opened must not
    /// stop the studio recording that a site went live.
    pub fn init() {
        let dir = cell_dir().cloned();

        match &dir {
            Some(path) => match Studio::open(path).or_else(|_| Studio::create(path)) {
                Ok(studio) => log::info!(
                    "handover cell ready at {}, id {}",
                    path.display(),
                    studio.cell().id()
                ),
                Err(error) => log::error!(
                    "handover cell at {} could not be opened: {error} — publishing will not sign",
                    path.display()
                ),
            },
            None => log::warn!(
                "HANDOVER_CELL_DIR is not set — publishing will record the handover \
                 in Postgres but sign nothing"
            ),
        }
    }

    /// Sign one `Released` claim per named artefact.
    ///
    /// Never fails the caller. Publication is already written by the time this
    /// runs, and an unsigned handover is a smaller problem than a site the
    /// studio cannot mark as live.
    pub async fn sign(customer_id: i64, artefacts: &Artefacts) -> Signed {
        let Some(dir) = cell_dir() else {
            log::error!("no handover cell configured — customer {customer_id} not signed for");
            return Signed::default();
        };

        let studio = match Studio::open(dir).or_else(|_| Studio::create(dir)) {
            Ok(s) => s,
            Err(error) => {
                log::error!("handover cell could not be opened: {error}");
                return Signed::default();
            }
        };

        let mut planned = Vec::new();
        if let Some(name) = &artefacts.domain {
            planned.push((Asset::Domain, Artefact::new(Asset::Domain, name)));
        }
        if let Some(reference) = &artefacts.hosting {
            planned.push((Asset::Hosting, Artefact::new(Asset::Hosting, reference)));
        }
        if let Some(url) = &artefacts.source {
            planned.push((Asset::SourceCode, Artefact::new(Asset::SourceCode, url)));
        }
        if planned.is_empty() {
            return Signed::default();
        }

        // Over-long references are refused by the dialect before anything is
        // signed; drop them rather than fail the whole handover, and say so.
        let mut to_sign = Vec::new();
        for (asset, artefact) in &planned {
            if artefact.fits() {
                to_sign.push(artefact.clone());
            } else {
                log::error!("{asset:?} reference is too long for a claim and was not signed");
            }
        }

        match studio.release_custody(customer_id, &to_sign) {
            Ok(claims) => {
                log::info!(
                    "signed {} handoff claim(s) for customer {customer_id}",
                    claims.len()
                );
                let named = |asset: Asset| to_sign.iter().any(|a| a.asset == asset);
                Signed {
                    domain: named(Asset::Domain),
                    hosting: named(Asset::Hosting),
                    source: named(Asset::SourceCode),
                }
            }
            Err(error) => {
                log::error!("handoff could not be signed: {error}");
                Signed::default()
            }
        }
    }
}

#[cfg(not(feature = "handover"))]
mod enabled {
    use super::{Artefacts, Signed};

    /// Nothing to set up when the feature is off.
    pub fn init() {}

    /// Never configured: without the feature there is nothing to sign with.
    #[cfg(test)]
    pub fn configured() -> bool {
        false
    }

    /// Signs nothing, and says so once at the point it would have.
    pub async fn sign(_customer_id: i64, _artefacts: &Artefacts) -> Signed {
        log::info!("built without the handover feature — nothing signed");
        Signed::default()
    }
}

#[cfg(test)]
pub use enabled::configured;
pub use enabled::{init, sign};
