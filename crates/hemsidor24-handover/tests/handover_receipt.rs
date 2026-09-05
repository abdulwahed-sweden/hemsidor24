//! End-to-end: sign a real handover into the studio's chain with the
//! first-party handoff dialect, read it back, and render the receipt.
#![cfg(feature = "handover")]

use hemsidor24_handover::{Asset, Event, JournalEntry, Studio, Transfer, receipt};

/// A cell in a temporary directory, removed when the test ends.
struct TempCell {
    dir: std::path::PathBuf,
}

impl TempCell {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir()
            .join("hemsidor24-handover-tests")
            .join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        TempCell { dir }
    }
}

impl Drop for TempCell {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn promised() -> Vec<Transfer> {
    vec![
        Transfer::new(Asset::Domain, "malmobygg.se"),
        Transfer::new(Asset::Hosting, "loopia:558812"),
        Transfer::new(Asset::SourceCode, "https://github.com/hemsidor24/malmobygg"),
    ]
}

#[test]
fn each_promised_asset_becomes_its_own_signed_claim() {
    let temp = TempCell::new("assets");
    let studio = Studio::create(&temp.dir).expect("cell created");

    let issued = studio
        .transfer_ownership(42, &promised())
        .expect("transferred");
    assert_eq!(issued.len(), 3, "the three promises move separately");

    let history = studio.history().expect("history");
    assert_eq!(history.len(), 3);

    for (_, _, body) in &history {
        assert_eq!(body.event, Event::Released, "the studio gives custody up");
        assert_eq!(
            body.counterparty.as_deref(),
            Some("cus-42"),
            "Released names the receiving party"
        );
        assert!(body.party.is_none(), "the cell speaks for itself");
    }

    let items: Vec<&str> = history.iter().map(|(_, _, b)| b.item.as_str()).collect();
    assert!(items.contains(&"dom:malmobygg.se"));
    assert!(items.contains(&"host:loopia:558812"));
    assert!(items.contains(&"src:https://github.com/hemsidor24/malmobygg"));
}

#[test]
fn no_customer_identity_reaches_a_signed_body() {
    let temp = TempCell::new("privacy");
    let studio = Studio::create(&temp.dir).expect("created");
    studio
        .transfer_ownership(42, &promised())
        .expect("transferred");

    for (_, _, body) in studio.history().expect("history") {
        let rendered = format!("{body:?}");
        for leaked in ["Malmö Bygg", "kontakt@", "070-", "Mansour"] {
            assert!(!rendered.contains(leaked), "signed body leaked {leaked:?}");
        }
    }
}

#[test]
fn a_discharge_names_the_release_it_closes() {
    let temp = TempCell::new("discharge");
    let studio = Studio::create(&temp.dir).expect("created");

    let domain = Transfer::new(Asset::Domain, "malmobygg.se");
    let released = studio
        .transfer_ownership(7, std::slice::from_ref(&domain))
        .expect("released");
    let release_id = released[0].id;

    let discharged = studio.discharge(&domain, release_id).expect("discharged");
    let body = &studio.history().expect("history")[1].2;

    assert_eq!(body.event, Event::Discharged);
    assert_eq!(
        body.acknowledges.as_ref(),
        Some(&release_id),
        "the pair can be checked instead of guessed at"
    );
    assert!(
        body.counterparty.is_none(),
        "the dialect refuses a counterparty on Discharged"
    );
    assert_eq!(discharged.claim.cell, released[0].claim.cell);
}

#[test]
fn a_transfer_naming_nothing_is_refused_before_signing() {
    let temp = TempCell::new("empty");
    let studio = Studio::create(&temp.dir).expect("created");
    assert!(studio.transfer_ownership(1, &[]).is_err());
    assert!(
        studio.history().expect("history").is_empty(),
        "nothing was signed"
    );
}

#[test]
fn claims_survive_closing_and_reopening_the_cell() {
    let temp = TempCell::new("reopen");
    {
        let studio = Studio::create(&temp.dir).expect("created");
        studio
            .transfer_ownership(1, &promised())
            .expect("transferred");
    }
    let studio = Studio::open(&temp.dir).expect("reopened");
    assert_eq!(studio.history().expect("history").len(), 3);
}

#[test]
fn altering_a_signed_claim_breaks_its_content_address() {
    let temp = TempCell::new("verify");
    let studio = Studio::create(&temp.dir).expect("created");
    studio
        .transfer_ownership(5, &promised())
        .expect("transferred");

    let history = studio.history().expect("history");
    let (_, signed, _) = &history[0];
    assert_eq!(signed.claim.id().expect("hashes"), signed.id);

    let mut tampered = signed.clone();
    tampered.claim.timestamp_ms += 1;
    assert_ne!(tampered.claim.id().expect("hashes"), tampered.id);
}

#[test]
fn the_receipt_carries_both_evidence_tiers_and_names_neither_protocol_nor_customer() {
    let temp = TempCell::new("receipt");
    let studio = Studio::create(&temp.dir).expect("created");
    studio
        .transfer_ownership(42, &promised())
        .expect("transferred");

    let journal = vec![
        JournalEntry::new("2026-09-04", "Förslag visat")
            .settling("Du ser förslaget innan sidan publiceras."),
        JournalEntry::new("2026-09-05", "Förslag godkänt")
            .settling("Ni betalar först när ni sett sidan."),
        JournalEntry::new("2026-09-06", "Revidering 1 använd").settling("En revidering ingår."),
    ];
    let signed = studio.history_for_customer(42).expect("history");
    let text = receipt::render(&studio.cell().id().to_string(), 42, &journal, &signed);

    for expected in [
        "ÖVERLÄMNINGSKVITTO",
        "Beställning:  #42",
        "Förslag visat",
        "Revidering 1 använd",
        "studiojournal",
        "Domän",
        "malmobygg.se",
        "signerat",
        "VAD DETTA INTE VISAR",
        "Kunden har inte undertecknat något här",
    ] {
        assert!(
            text.contains(expected),
            "receipt missing {expected:?}:\n{text}"
        );
    }

    // The customer's own reference is an internal code; it has no place on a
    // document handed to that same customer.
    assert!(
        !text.contains("cus-42"),
        "internal reference leaked onto the receipt"
    );
}
