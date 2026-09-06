//! Proving the operator's backup and restore procedure actually works.
//!
//! The studio's cell is the one thing here that cannot be regenerated. A
//! Postgres backup does not cover it: Postgres holds commercial state, where a
//! lost row is a lost record, while the cell holds a signing identity and an
//! append-only chain. A replacement key is a different cell, and a chain
//! restored "mostly" is not restored at all, because its whole value is that
//! nothing was taken out of the middle.
//!
//! So the procedure is a test rather than a paragraph. It walks exactly what an
//! operator does — create deliberately, record the id, stop, copy the whole
//! directory, restore into a clean one, reopen, check the id, verify the claims
//! still there, and append to prove the restored cell is a working cell and not
//! just a readable one.
#![cfg(feature = "handover")]

use hemsidor24_handover::{Artefact, Asset, Studio};

/// A directory that removes itself. Cells written here hold a real signing
/// key, so leaving one behind after a failed test is not acceptable.
struct TempDir {
    dir: std::path::PathBuf,
}

impl TempDir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir()
            .join("hemsidor24-cell-backup")
            .join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        TempDir { dir }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Copy every file in a cell directory, the way an operator's backup must.
///
/// Deliberately whole-directory. The key without the log is an identity with
/// no history; the log without the key is a record that can never be added to.
fn copy_whole_directory(from: &std::path::Path, to: &std::path::Path) -> Vec<String> {
    std::fs::create_dir_all(to).expect("destination");
    let mut copied = Vec::new();
    for entry in std::fs::read_dir(from).expect("read cell dir") {
        let entry = entry.expect("entry");
        if entry.file_type().expect("file type").is_file() {
            let name = entry.file_name().to_string_lossy().into_owned();
            std::fs::copy(entry.path(), to.join(&name)).expect("copy");
            copied.push(name);
        }
    }
    copied.sort();
    copied
}

#[test]
fn a_backed_up_cell_restores_to_the_same_identity_and_keeps_working() {
    let live = TempDir::new("live");
    let backup = TempDir::new("backup");
    let restored = TempDir::new("restored");

    // 1. Create the cell deliberately, and record what it signed.
    let original_id;
    let original_history;
    {
        let studio = Studio::create(&live.dir).expect("cell created");
        original_id = studio.cell().id().to_string();

        studio
            .release_custody(
                42,
                &[
                    Artefact::new(Asset::Domain, "malmobygg.se"),
                    Artefact::new(Asset::Hosting, "loopia:558812"),
                ],
            )
            .expect("claims signed");

        original_history = studio.history().expect("history");
        assert_eq!(original_history.len(), 2, "two claims were signed");
        // 2. Stop the application. A cell directory takes one holder at a
        //    time, so the backup is taken with nothing holding it.
    }

    // 3. Back up the whole directory as one unit.
    let files = copy_whole_directory(&live.dir, &backup.dir);
    assert!(
        files.contains(&"cell.key".to_owned())
            && files.contains(&"log.sijill".to_owned())
            && files.contains(&"log.sijill.horizon".to_owned()),
        "a backup must take the key, the log and the horizon together, got {files:?}"
    );

    // 4. Restore into a clean directory, as it would be on a new machine.
    let restored_files = copy_whole_directory(&backup.dir, &restored.dir);
    assert_eq!(
        files, restored_files,
        "the restore is byte-for-byte complete"
    );

    // 5. Open the restored cell. The original is never opened at the same
    //    time — nothing above still holds it.
    let restored_studio = Studio::open(&restored.dir).expect("restored cell opens");

    // 6. Same identity. This is the whole point: a cell that came back with a
    //    different id would be a different cell wearing the old one's data.
    assert_eq!(
        original_id,
        restored_studio.cell().id().to_string(),
        "the restored cell must be the same cell"
    );

    // 7. The claims that were there are still there, and still readable.
    let restored_history = restored_studio.history().expect("history after restore");
    assert_eq!(
        original_history.len(),
        restored_history.len(),
        "no claim was lost in the round trip"
    );
    for (before, after) in original_history.iter().zip(&restored_history) {
        assert_eq!(
            format!("{before:?}"),
            format!("{after:?}"),
            "every restored entry matches what was signed"
        );
    }
    let signed = restored_studio
        .cell()
        .claims(&restored_studio.cell().id())
        .expect("claims verify against the restored cell");
    assert_eq!(signed.len(), 2, "both signed claims verify after restore");

    // 8. And it is a working cell, not merely a readable one: it can extend
    //    the chain it came back with.
    restored_studio
        .release_custody(
            43,
            &[Artefact::new(
                Asset::SourceCode,
                "https://github.com/hemsidor24/nytt",
            )],
        )
        .expect("the restored cell can still sign");
    assert_eq!(
        restored_studio.history().expect("history").len(),
        3,
        "the new claim joined the restored chain"
    );
}

#[test]
fn a_partial_restore_does_not_announce_itself() {
    // The failure an operator is most likely to cause: backing up "the data"
    // and leaving the key, or the key and not the data.
    //
    // The dangerous half is the key on its own. It opens, and it opens as the
    // *right cell* — same id, no error, nothing in any log to suggest a
    // problem. Only the claims are gone. An operator who restores a backup and
    // checks the id alone would call that a successful restore and would be
    // wrong, which is why the procedure checks the claim count as well.
    let live = TempDir::new("partial-live");
    let key_only = TempDir::new("partial-key");
    let log_only = TempDir::new("partial-log");

    let original_id;
    {
        let studio = Studio::create(&live.dir).expect("created");
        original_id = studio.cell().id().to_string();
        studio
            .release_custody(7, &[Artefact::new(Asset::Domain, "exempel.se")])
            .expect("signed");
        assert_eq!(studio.history().expect("history").len(), 1);
    }

    std::fs::copy(live.dir.join("cell.key"), key_only.dir.join("cell.key")).expect("copy key");
    let half = Studio::open(&key_only.dir).expect("a key on its own still opens");
    assert_eq!(
        half.cell().id().to_string(),
        original_id,
        "and it presents the identity of the cell it came from"
    );
    assert!(
        half.history().expect("history").is_empty(),
        "while every claim it was signing for is gone"
    );
    drop(half);

    // The other half fails outright, which is the safer of the two failures.
    for name in ["log.sijill", "log.sijill.horizon"] {
        std::fs::copy(live.dir.join(name), log_only.dir.join(name)).expect("copy log");
    }
    assert!(
        Studio::open(&log_only.dir).is_err(),
        "a log with no key cannot be opened at all"
    );
}
