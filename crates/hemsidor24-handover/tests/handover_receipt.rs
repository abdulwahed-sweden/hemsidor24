//! End-to-end: create the studio's cell, sign one job's worth of claims, read
//! them back, verify them, and render the receipt.
//!
//! Only compiled when the `handover` feature is on.
#![cfg(feature = "handover")]

use hemsidor24_core::Package;
use hemsidor24_handover::{Body, Event, Studio, receipt};

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

/// One job, start to finish: proposal shown, accepted, a revision spent, then
/// the domain and source handed over.
#[test]
fn a_whole_job_is_signed_read_back_and_rendered() {
    let temp = TempCell::new("whole-job");
    let studio = Studio::create(&temp.dir).expect("cell created");

    let order = 42u64;
    let site = 7u64;

    studio.offer_delivery(order, site).expect("offered");
    studio.accept_delivery(order, site).expect("accepted");
    studio.use_revision(order, site, 1).expect("revision");
    studio
        .transfer_ownership(
            order,
            site,
            "malmobygg.se",
            "https://github.com/hemsidor24/malmobygg",
        )
        .expect("transferred");

    let history = studio.history_for_order(order).expect("history");
    assert_eq!(history.len(), 4, "every claim should come back");

    let events: Vec<Event> = history.iter().map(|(_, _, body)| body.event).collect();
    assert_eq!(
        events,
        [
            Event::DeliveryOffered,
            Event::DeliveryAccepted,
            Event::RevisionUsed,
            Event::OwnershipTransferred,
        ],
        "and in the order they were signed"
    );

    let transfer = &history[3].2;
    assert_eq!(transfer.domain, "malmobygg.se");
    assert_eq!(transfer.repo_url, "https://github.com/hemsidor24/malmobygg");
    assert_eq!(transfer.site_ref, site);

    let text = receipt::render(&studio.cell().id().to_string(), order, &history);
    for expected in [
        "ÖVERLÄMNINGSKVITTO",
        "Beställning:  #42",
        "Förslag visat",
        "Förslag godkänt",
        "Revidering använd",
        "Äganderätt överförd",
        "malmobygg.se",
        "https://github.com/hemsidor24/malmobygg",
        "VAD DETTA INTE VISAR",
        "Kunden driver ingen egen cell",
    ] {
        assert!(
            text.contains(expected),
            "receipt is missing {expected:?}:\n{text}"
        );
    }
}

/// The chain is the studio's own, and reopening it must not lose anything.
#[test]
fn claims_survive_closing_and_reopening_the_cell() {
    let temp = TempCell::new("reopen");
    {
        let studio = Studio::create(&temp.dir).expect("created");
        studio.offer_delivery(1, 1).expect("offered");
        studio.accept_delivery(1, 1).expect("accepted");
    }
    let studio = Studio::open(&temp.dir).expect("reopened");
    assert_eq!(studio.history().expect("history").len(), 2);
}

/// A refund is priced from the package, not from a number typed at the call
/// site, so the receipt cannot quote an amount the customer was never charged.
#[test]
fn a_refund_records_the_price_the_customer_was_quoted() {
    let temp = TempCell::new("refund");
    let studio = Studio::create(&temp.dir).expect("created");

    studio
        .refund(9, Package::Start, "Kunden nöjd ej före publicering")
        .expect("refunded");
    studio
        .refund(10, Package::Pro, "Ångrade sig")
        .expect("refunded");

    let history = studio.history().expect("history");
    assert_eq!(
        history[0].2.refund_ore,
        u64::from(Package::Start.price_ore_inc_vat())
    );
    assert_eq!(
        history[1].2.refund_ore,
        u64::from(Package::Pro.price_ore_inc_vat())
    );

    let text = receipt::render("cell", 9, &studio.history_for_order(9).expect("h"));
    assert!(text.contains("3112,50 kr"), "{text}");
}

/// Claims about one order must not leak into another order's receipt.
#[test]
fn a_receipt_covers_one_order_only() {
    let temp = TempCell::new("scoped");
    let studio = Studio::create(&temp.dir).expect("created");

    studio.offer_delivery(1, 1).expect("offered");
    studio
        .transfer_ownership(2, 2, "annan.se", "https://github.com/x/annan")
        .expect("other");

    let first = studio.history_for_order(1).expect("history");
    assert_eq!(first.len(), 1);

    let text = receipt::render("cell", 1, &first);
    assert!(
        !text.contains("annan.se"),
        "another order's domain leaked in:\n{text}"
    );
}

/// A record's stored identifier is the hash of its content, so altering the
/// content makes the two disagree. This is the check `SignedClaim::verify`
/// performs before it looks at the signature at all.
#[test]
fn altering_a_signed_claim_breaks_its_content_address() {
    let temp = TempCell::new("verify");
    let studio = Studio::create(&temp.dir).expect("created");
    studio
        .transfer_ownership(5, 5, "exempel.se", "https://github.com/x/exempel")
        .expect("transferred");

    let history = studio.history().expect("history");
    let (_, signed, _) = &history[0];

    // Untouched: the stored id is the content address.
    assert_eq!(signed.claim.id().expect("hashes"), signed.id);

    // Move the timestamp by one millisecond and they no longer agree.
    let mut tampered = signed.clone();
    tampered.claim.timestamp_ms += 1;
    assert_ne!(
        tampered.claim.id().expect("hashes"),
        tampered.id,
        "altered content must not keep its identifier"
    );

    // Same for the body: rewrite the domain that was handed over.
    let mut swapped = signed.clone();
    if let Body::Inline(bytes) = &mut swapped.claim.body {
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
    }
    assert_ne!(swapped.claim.id().expect("hashes"), swapped.id);
}
