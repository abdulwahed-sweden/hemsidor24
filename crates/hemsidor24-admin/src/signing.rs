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

    /// Set once, deliberately, to bring a brand-new cell into existence.
    ///
    /// Creating a cell is not something that should ever happen by accident.
    /// A cell that is created rather than opened has a **new signing identity
    /// and an empty log**, so every claim the studio signed before it is
    /// orphaned: still valid, but no longer part of the chain this cell will
    /// extend. Requiring an explicit variable means that can only happen when
    /// somebody asked for it.
    const INIT_VAR: &str = "HANDOVER_CELL_INIT";

    /// Decide whether signing can work, or why the process must not start.
    ///
    /// Split out from [`init`] so the outcomes can be tested without touching
    /// process-wide environment or the `OnceLock` behind [`cell_dir`].
    fn check(
        dir: Option<&std::path::Path>,
        may_create: bool,
    ) -> Result<Studio, crate::StartupError> {
        let Some(path) = dir else {
            return Err(crate::StartupError::Missing("HANDOVER_CELL_DIR"));
        };

        let studio = match Studio::open(path) {
            Ok(studio) => studio,
            // Nothing openable there. Creating one silently is exactly the
            // failure this guards: a redeploy onto an empty volume would mint
            // a fresh identity, sign with it, and report success.
            Err(_) if !may_create => {
                return Err(crate::StartupError::Invalid {
                    var: "HANDOVER_CELL_DIR",
                    reason: concat!(
                        "holds no cell that could be opened. Restore the cell ",
                        "directory from backup, or set HANDOVER_CELL_INIT=1 once ",
                        "to create a new cell with a new signing identity."
                    ),
                });
            }
            Err(_) => Studio::create(path).map_err(|_| crate::StartupError::Invalid {
                var: "HANDOVER_CELL_DIR",
                reason: "no cell could be created there",
            })?,
        };

        // Opening proves the key can be read. Signing also appends to the log,
        // so a directory mounted read-only would open here and fail at the one
        // moment that matters.
        let probe = path.join(".hemsidor24-write-probe");
        std::fs::write(&probe, b"").map_err(|_| crate::StartupError::Invalid {
            var: "HANDOVER_CELL_DIR",
            reason: "is not writable, so claims could be signed but never recorded",
        })?;
        let _ = std::fs::remove_file(&probe);

        Ok(studio)
    }

    /// Prove signing works before the process serves anything.
    ///
    /// Compiling the feature in is a statement that this deployment signs its
    /// handovers. Starting anyway with signing broken would mean publishing
    /// looked successful while producing no record at all — the operator would
    /// find out when they went looking for a claim that was never made. So
    /// this is fatal, not a warning.
    pub fn init() -> Result<(), crate::StartupError> {
        let may_create = std::env::var(INIT_VAR).is_ok_and(|v| !v.trim().is_empty());
        let studio = check(cell_dir().map(std::path::PathBuf::as_path), may_create)?;
        log::info!(
            "handover cell ready, id {} — signing is active",
            studio.cell().id()
        );
        Ok(())
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

        // Open only. Startup already proved this works, so a failure here is a
        // real anomaly — and creating a replacement would hide it behind a new
        // identity rather than surface it.
        let studio = match Studio::open(dir) {
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
    #[cfg(test)]
    mod tests {
        use super::*;

        /// A directory that removes itself, so a failed test leaves no cell
        /// lying around with a real signing key in it.
        struct TempDir(std::path::PathBuf);

        impl TempDir {
            fn new(tag: &str) -> Self {
                let mut path = std::env::temp_dir();
                path.push(format!("hemsidor24-cell-{tag}-{}", std::process::id()));
                let _ = std::fs::remove_dir_all(&path);
                std::fs::create_dir_all(&path).expect("temp dir");
                TempDir(path)
            }
        }

        impl Drop for TempDir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        #[test]
        fn a_cell_directory_takes_one_holder_at_a_time() {
            // Not a rule this code imposes — it is how the cell behaves, and it
            // decides how the back office may be deployed. Two admin processes
            // pointed at one cell directory cannot both sign, so a rolling
            // restart that overlaps the old and new process will leave one
            // publication unsigned. Deploy this stop-then-start, one instance.
            let dir = TempDir::new("single");
            let held = Studio::create(&dir.0).expect("created");
            assert!(
                Studio::open(&dir.0).is_err(),
                "a second holder must not get the same cell"
            );
            drop(held);
            assert!(
                Studio::open(&dir.0).is_ok(),
                "and it is available again once released"
            );
        }

        #[test]
        fn startup_fails_when_no_cell_directory_is_configured() {
            let Err(error) = check(None, false) else {
                panic!("must not start with no cell directory")
            };
            assert!(
                matches!(error, crate::StartupError::Missing("HANDOVER_CELL_DIR")),
                "{error}"
            );
        }

        #[test]
        fn startup_fails_when_the_directory_holds_no_cell() {
            // The redeploy-onto-an-empty-volume case. Before this was fatal it
            // minted a new identity, signed with it, and logged INFO.
            let dir = TempDir::new("empty");
            let Err(error) = check(Some(&dir.0), false) else {
                panic!("must not start on a directory holding no cell")
            };
            assert!(
                matches!(
                    error,
                    crate::StartupError::Invalid {
                        var: "HANDOVER_CELL_DIR",
                        ..
                    }
                ),
                "{error}"
            );
        }

        #[test]
        fn startup_fails_when_the_path_is_not_a_usable_directory() {
            let dir = TempDir::new("notadir");
            let file = dir.0.join("cell-that-is-a-file");
            std::fs::write(&file, b"not a cell").expect("write");
            let Err(error) = check(Some(&file), false) else {
                panic!("must not start on a path that is not a cell directory")
            };
            assert!(
                matches!(
                    error,
                    crate::StartupError::Invalid {
                        var: "HANDOVER_CELL_DIR",
                        ..
                    }
                ),
                "{error}"
            );
        }

        #[test]
        fn startup_succeeds_on_a_cell_that_already_exists() {
            let dir = TempDir::new("valid");
            let created = Studio::create(&dir.0).expect("cell created");
            let expected = created.cell().id().to_string();
            drop(created);

            let studio = check(Some(&dir.0), false).expect("starts");
            assert_eq!(
                studio.cell().id().to_string(),
                expected,
                "startup opens the existing cell rather than replacing it"
            );
        }

        #[test]
        fn an_empty_directory_is_only_turned_into_a_cell_when_asked() {
            let dir = TempDir::new("optin");
            assert!(
                check(Some(&dir.0), false).is_err(),
                "refuses to create without the opt-in"
            );
            assert!(
                !dir.0.join("cell.key").exists(),
                "a refused start leaves no key behind"
            );

            let studio = check(Some(&dir.0), true).expect("creates when asked");
            assert!(dir.0.join("cell.key").exists());
            let created = studio.cell().id().to_string();
            // A cell directory takes one handle at a time, so release this one
            // before asking for another. See the test below.
            drop(studio);

            // And having created one, an ordinary start must now open that
            // same cell rather than make another.
            let reopened = check(Some(&dir.0), false).expect("opens what it made");
            assert_eq!(created, reopened.cell().id().to_string());
        }
    }
}

#[cfg(not(feature = "handover"))]
mod enabled {
    use super::{Artefacts, Signed};

    /// Nothing to set up when the feature is off, and nothing that can fail.
    pub fn init() -> Result<(), crate::StartupError> {
        Ok(())
    }

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
