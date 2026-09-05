//! Print the claims in an existing cell.
//!
//! ```sh
//! HANDOVER_CELL_DIR=/path/to/cell \
//!   cargo run -p hemsidor24-handover --features handover --example inspect
//! ```
//!
//! For looking at what a running back office actually signed.

use hemsidor24_handover::Studio;
use hemsidor24_handover::studio::{asset_of, identifier_of};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::var("HANDOVER_CELL_DIR")
        .map_err(|_| "set HANDOVER_CELL_DIR to the cell's directory")?;
    let studio = Studio::open(&dir)?;

    println!("cell {}", studio.cell().id());
    for (seq, claim, body) in studio.history()? {
        let what = asset_of(&body.item).map_or("—", |a| a.label_sv());
        println!(
            "  [{seq}] {:<10} {:<12} {}\n       motpart: {}\n       {}",
            body.event.to_string(),
            what,
            identifier_of(&body.item),
            body.counterparty.clone().unwrap_or_else(|| "—".to_owned()),
            claim.id
        );
    }
    Ok(())
}
