//! Print a handover receipt for one made-up job.
//!
//! ```sh
//! cargo run -p hemsidor24-handover --features handover --example receipt
//! ```
//!
//! The journal lines stand in for rows the back office would read out of
//! Postgres; the ownership lines are really signed, into a throwaway cell that
//! is removed afterwards.

use hemsidor24_handover::{Asset, JournalEntry, Studio, Transfer, receipt};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join(format!("hemsidor24-receipt-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;

    let studio = Studio::create(&dir)?;
    let (order, customer) = (42, 42);

    let promised = [
        Transfer::new(Asset::Domain, "malmobygg.se"),
        Transfer::new(Asset::Hosting, "loopia:558812"),
        Transfer::new(Asset::SourceCode, "https://github.com/hemsidor24/malmobygg"),
    ];
    studio.transfer_ownership(customer, &promised)?;

    let journal = vec![
        JournalEntry::new("2026-09-04 09:12", "Förslag visat")
            .settling("Du ser förslaget innan sidan publiceras."),
        JournalEntry::new("2026-09-05 14:03", "Förslag godkänt")
            .settling("Ni betalar först när ni sett sidan."),
        JournalEntry::new("2026-09-06 10:41", "Revidering 1 använd")
            .settling("En revidering ingår."),
    ];

    let signed = studio.history_for_customer(customer)?;
    print!(
        "{}",
        receipt::render(&studio.cell().id().to_string(), order, &journal, &signed)
    );

    std::fs::remove_dir_all(&dir)?;
    Ok(())
}
