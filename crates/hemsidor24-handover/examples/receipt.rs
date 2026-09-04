//! Print a handover receipt for one made-up job.
//!
//! ```sh
//! cargo run -p hemsidor24-handover --features handover --example receipt
//! ```
//!
//! Writes its cell to a temporary directory and removes it afterwards, so it
//! never touches a real one.

use hemsidor24_handover::{Studio, receipt};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join(format!("hemsidor24-receipt-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;

    let studio = Studio::create(&dir)?;
    let (order, site) = (42, 7);

    studio.offer_delivery(order, site)?;
    studio.accept_delivery(order, site)?;
    studio.use_revision(order, site, 1)?;
    studio.transfer_ownership(
        order,
        site,
        "malmobygg.se",
        "https://github.com/hemsidor24/malmobygg",
    )?;

    let history = studio.history_for_order(order)?;
    print!(
        "{}",
        receipt::render(&studio.cell().id().to_string(), order, &history)
    );

    std::fs::remove_dir_all(&dir)?;
    Ok(())
}
