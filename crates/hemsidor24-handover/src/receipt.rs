//! The handover receipt.
//!
//! A plain-text rendering of what the studio signed about one order: what was
//! promised, what was recorded, when, and by which key. Written to be handed to
//! a lawyer, which means it has to say what it does *not* establish as plainly
//! as what it does — a document that overstates itself is worse than none.

use sijill_cell::format_timestamp;
use sijill_core::SignedClaim;

use crate::claim::{Event, HandoverClaim};

/// What this receipt cannot establish, in the words of the protocol's own
/// limitations. Reproduced on every receipt so it cannot be separated from the
/// claims it qualifies.
const CAVEATS_SV: &[&str] = &[
    "Detta visar vad Hemsidor24 har undertecknat, och att texten inte har \
     ändrats sedan dess. Det visar inte att innehållet är sant.",
    "Kunden driver ingen egen cell och har inte undertecknat något här. \
     Varje post ovan är Hemsidor24:s egen uppgift, inte ett ömsesidigt utbyte.",
    "Tidpunkterna är angivna av Hemsidor24 själv och styrks inte av någon \
     utomstående.",
    "Att nyckeln ovan tillhör Hemsidor24 är en bedömning läsaren gör; \
     det framgår inte av dokumentet i sig.",
];

/// Render the receipt for one order.
///
/// `claims` should be that order's history, oldest first, as
/// [`crate::Studio::history_for_order`] returns it.
pub fn render(
    cell_id: &str,
    order_ref: u64,
    claims: &[(u64, SignedClaim, HandoverClaim)],
) -> String {
    let mut out = String::new();

    out.push_str("ÖVERLÄMNINGSKVITTO\n");
    out.push_str("Hemsidor24\n\n");
    out.push_str(&format!("Beställning:  #{order_ref}\n"));
    out.push_str(&format!("Cell:         {cell_id}\n"));
    out.push_str(&format!("Poster:       {}\n\n", claims.len()));

    if claims.is_empty() {
        out.push_str("Inga undertecknade poster för denna beställning.\n\n");
    } else {
        out.push_str("HÄNDELSER\n\n");
        for (seq, signed, body) in claims {
            out.push_str(&format!("  [{seq}] {}\n", body.event.label_sv()));
            out.push_str(&format!(
                "       Tid (enligt Hemsidor24):  {}\n",
                format_timestamp(signed.claim.timestamp_ms)
            ));
            out.push_str(&format!("       Post-id:                  {}\n", signed.id));
            if body.site_ref != 0 {
                out.push_str(&format!(
                    "       Sida:                     #{}\n",
                    body.site_ref
                ));
            }
            match body.event {
                Event::OwnershipTransferred => {
                    if !body.domain.is_empty() {
                        out.push_str(&format!(
                            "       Domän:                    {}\n",
                            body.domain
                        ));
                    }
                    if !body.repo_url.is_empty() {
                        out.push_str(&format!(
                            "       Källkod:                  {}\n",
                            body.repo_url
                        ));
                    }
                }
                Event::RevisionUsed => {
                    out.push_str(&format!(
                        "       Revidering nr:            {}\n",
                        body.revision
                    ));
                }
                Event::RefundIssued => {
                    out.push_str(&format!(
                        "       Belopp:                   {} kr\n",
                        format_ore(body.refund_ore)
                    ));
                }
                Event::DeliveryOffered | Event::DeliveryAccepted => {}
            }
            out.push_str(&format!(
                "       Löfte:                    {}\n",
                body.event.promise_sv()
            ));
            if !body.note.is_empty() {
                out.push_str(&format!("       Notering:                 {}\n", body.note));
            }
            out.push('\n');
        }
    }

    out.push_str("VAD DETTA INTE VISAR\n\n");
    for caveat in CAVEATS_SV {
        out.push_str("  - ");
        // Wrap so the document reads on paper.
        out.push_str(&wrap(caveat, 72, "    "));
        out.push('\n');
    }

    out
}

/// Öre as kronor, with two decimals and a comma, the Swedish way.
fn format_ore(ore: u64) -> String {
    format!("{},{:02}", ore / 100, ore % 100)
}

/// Wrap `text` to `width`, indenting continuation lines with `indent`.
fn wrap(text: &str, width: usize, indent: &str) -> String {
    let mut out = String::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            out.push_str(&line);
            out.push('\n');
            out.push_str(indent);
            line.clear();
        } else if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    out.push_str(&line);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ore_render_as_swedish_kronor() {
        assert_eq!(format_ore(311_250), "3112,50");
        assert_eq!(format_ore(561_250), "5612,50");
        assert_eq!(format_ore(0), "0,00");
        assert_eq!(format_ore(5), "0,05");
    }

    #[test]
    fn an_empty_history_still_renders_a_document() {
        let text = render("cell-abc", 42, &[]);
        assert!(text.contains("Beställning:  #42"));
        assert!(text.contains("Inga undertecknade poster"));
    }

    #[test]
    fn the_caveats_are_always_present() {
        // They qualify the claims, so they must never be separable from them.
        let text = render("cell-abc", 1, &[]);
        assert!(text.contains("VAD DETTA INTE VISAR"));
        assert!(text.contains("Det visar inte att innehållet är sant"));
        assert!(text.contains("Kunden driver ingen egen cell"));
    }

    #[test]
    fn wrapping_keeps_lines_readable_on_paper() {
        let text = render("cell-abc", 1, &[]);
        for line in text.lines() {
            assert!(line.chars().count() <= 90, "line too long: {line:?}");
        }
    }
}
