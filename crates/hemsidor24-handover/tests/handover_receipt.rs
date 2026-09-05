//! End-to-end: sign a real handover into the studio's chain with the
//! first-party handoff dialect, read it back, and render the receipt.
#![cfg(feature = "handover")]

use hemsidor24_handover::{Artefact, Asset, Event, JournalEntry, Studio, receipt};

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

fn promised() -> Vec<Artefact> {
    vec![
        Artefact::new(Asset::Domain, "malmobygg.se"),
        Artefact::new(Asset::Hosting, "loopia:558812"),
        Artefact::new(Asset::SourceCode, "https://github.com/hemsidor24/malmobygg"),
    ]
}

#[test]
fn each_promised_artefact_becomes_its_own_signed_claim() {
    let temp = TempCell::new("assets");
    let studio = Studio::create(&temp.dir).expect("cell created");

    let issued = studio
        .release_custody(42, &promised())
        .expect("transferred");
    assert_eq!(issued.len(), 3, "the three artefacts move separately");

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
        .release_custody(42, &promised())
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

    let domain = Artefact::new(Asset::Domain, "malmobygg.se");
    let released = studio
        .release_custody(7, std::slice::from_ref(&domain))
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
fn a_release_naming_nothing_is_refused_before_signing() {
    let temp = TempCell::new("empty");
    let studio = Studio::create(&temp.dir).expect("created");
    assert!(studio.release_custody(1, &[]).is_err());
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
        studio.release_custody(1, &promised()).expect("transferred");
    }
    let studio = Studio::open(&temp.dir).expect("reopened");
    assert_eq!(studio.history().expect("history").len(), 3);
}

#[test]
fn altering_a_signed_claim_breaks_its_content_address() {
    let temp = TempCell::new("verify");
    let studio = Studio::create(&temp.dir).expect("created");
    studio.release_custody(5, &promised()).expect("transferred");

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
        .release_custody(42, &promised())
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

// ---- Wording contract -------------------------------------------------------
//
// The governing rule: the receipt must never claim more than the signed claims
// actually prove. A `Released` signed by the studio proves the studio asserted
// it gave up control. It does not prove title moved, and it does not prove the
// customer did anything at all.

/// A realistic receipt, signed claims and journal both present.
///
/// Takes a name because each test needs its own cell: they run in parallel and
/// a cell directory is exclusively locked while it is open.
fn rendered(name: &str) -> String {
    let temp = TempCell::new(name);
    let studio = Studio::create(&temp.dir).expect("created");
    let artefacts = promised();
    let released = studio.release_custody(42, &artefacts).expect("released");
    studio
        .discharge(&artefacts[0], released[0].id)
        .expect("discharged");

    let journal = vec![
        JournalEntry::new("2026-09-04", "Förslag visat")
            .settling("Du ser förslaget innan sidan publiceras."),
        JournalEntry::new("2026-09-06", "Revidering 1 använd").settling("En revidering ingår."),
    ];
    let signed = studio.history_for_customer(42).expect("history");
    receipt::render(&studio.cell().id().to_string(), 42, &journal, &signed)
}

/// The section between `ÖVERLÄMNING` and the next heading — the signed rows,
/// and nothing else.
fn signed_section(receipt: &str) -> String {
    let start = receipt
        .find("ÖVERLÄMNING\n")
        .expect("the signed section exists");
    let rest = &receipt[start + "ÖVERLÄMNING\n".len()..];
    let end = rest.find("VAD SOM UTLOVADES").unwrap_or(rest.len());
    rest[..end].to_string()
}

#[test]
fn the_signed_section_uses_no_legal_ownership_wording() {
    // Scoped to the signed rows on purpose. The word "äganderätt" does appear
    // elsewhere in the document — once in the quoted promise, which is framed
    // as what was promised rather than what is proven, and once in the caveat
    // that explicitly denies title moved. Banning it there would be banning
    // the disclaimer.
    let section = signed_section(&rendered("w-ownership")).to_lowercase();
    for word in [
        "äganderätt",
        "ägande",
        "ägare",
        "äger",
        "överförd till kund",
        "ownership",
    ] {
        assert!(
            !section.contains(word),
            "the signed section claims ownership with {word:?}"
        );
    }
    assert!(section.contains("studion lämnade ifrån sig kontrollen"));
}

#[test]
fn the_document_denies_title_transfer_explicitly() {
    let text = rendered("w-title");
    assert!(text.contains("inte att kunden tagit emot den"));
    assert!(text.contains("äganderätt eller upphovsrätt har övergått"));
}

#[test]
fn the_receipt_never_says_the_customer_received_anything() {
    // There is no customer cell, so no `Received` claim can exist. Until one
    // does, nothing may be rendered as an act by the customer.
    let text = rendered("w-received").to_lowercase();
    for phrase in [
        "kunden har tagit emot",
        "kunden tog emot",
        "mottagen av kund",
        "kunden godkände överlämningen",
        "kunden bekräftade",
    ] {
        assert!(
            !text.contains(phrase),
            "receipt speaks for the customer: {phrase:?}"
        );
    }
    // What it may say is what the studio did.
    assert!(text.contains("studion lämnade ifrån sig kontrollen"));
}

#[test]
fn discharged_is_not_rendered_as_release_of_liability() {
    let text = rendered("w-discharged").to_lowercase();
    for phrase in [
        "ansvar avslutat",
        "ansvarsfrihet",
        "friskrivning",
        "avtalet avslutat",
    ] {
        assert!(
            !text.contains(phrase),
            "Discharged overclaimed as {phrase:?}"
        );
    }
    assert!(text.contains("studion avslutade sin journal"));
}

#[test]
fn the_registrar_caveat_does_not_promise_legal_title() {
    let text = rendered("w-registrar");
    assert!(text.contains("kontrollerar ett konto eller en domän"));
    assert!(text.contains("avgörs inte av detta dokument"));
}

#[test]
fn the_two_evidence_tiers_stay_distinguishable() {
    let text = rendered("w-tiers");
    assert!(text.contains("Underlag:  studiojournal"));
    assert!(text.contains("Underlag:  signerat av Hemsidor24"));
}

#[test]
fn customer_identity_still_cannot_reach_a_signed_body_or_the_receipt() {
    let temp = TempCell::new("pii");
    let studio = Studio::create(&temp.dir).expect("created");
    studio.release_custody(42, &promised()).expect("released");
    for (_, _, body) in studio.history().expect("history") {
        let rendered = format!("{body:?}");
        for leaked in ["Malmö Bygg", "kontakt@", "070-", "Mansour"] {
            assert!(!rendered.contains(leaked), "signed body leaked {leaked:?}");
        }
    }
    assert!(
        !rendered("w-pii-receipt").contains("cus-42"),
        "internal reference leaked onto the receipt"
    );
}

#[test]
fn protocol_vocabulary_stays_out_of_customer_facing_copy() {
    let text = rendered("w-vocab").to_lowercase();
    for word in [
        "dialekt",
        "dialect",
        "kanonisk",
        "encoder",
        "content address",
        "kedja",
        "chain",
    ] {
        assert!(
            !text.contains(word),
            "receipt leaked protocol vocabulary: {word:?}"
        );
    }
}
