//! The handover receipt.
//!
//! One document, two kinds of evidence, and it says which is which on every
//! line — because pretending they are the same strength would be the one
//! dishonesty that matters here.
//!
//! **Studiojournal.** What the studio recorded as it worked: proposal shown,
//! proposal accepted, revision spent, refund issued. These live in Postgres
//! under the back office's audit trail. They are ordinary business records —
//! good ones, attributable and timestamped, but held and editable by the
//! studio.
//!
//! **Signerat.** The studio releasing control of an artefact. Signed into an
//! append-only chain, tamper-evident, and checkable by someone who does not
//! trust the studio's database.
//!
//! A signed row says what the *studio* did, and stops there. There is no
//! customer cell and therefore no `Received`, so the receipt must never
//! render the customer as having taken, accepted or agreed to anything, and
//! must never present a release of control as a transfer of legal title.
//!
//! The reader never meets the words dialect, cell, claim or content address.
//! They meet a date, a thing, and a note saying how firmly it is established.

use sijill_cell::format_timestamp;

use crate::studio::{Entry, asset_of, identifier_of};

/// Something the studio recorded while working, carried in from Postgres.
///
/// This crate does not talk to a database; the caller fills these from the
/// `deliveries` and `sites` rows. That keeps the handover crate isolated, and
/// keeps the receipt honest about where each line came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalEntry {
    /// When it happened, as the studio recorded it.
    pub when: String,
    /// What happened, in the words the customer already knows.
    pub what: String,
    /// The promise it settles, if it settles one.
    pub promise: Option<String>,
}

impl JournalEntry {
    /// Record one studio-journal line.
    pub fn new(when: impl Into<String>, what: impl Into<String>) -> Self {
        JournalEntry {
            when: when.into(),
            what: what.into(),
            promise: None,
        }
    }

    /// Name the promise this line settles.
    pub fn settling(mut self, promise: impl Into<String>) -> Self {
        self.promise = Some(promise.into());
        self
    }
}

/// What the receipt cannot establish, in the protocol's own terms.
///
/// Printed on every receipt so it cannot be separated from the claims it
/// qualifies.
const CAVEATS_SV: &[&str] = &[
    "Rader märkta \"signerat\" är undertecknade av Hemsidor24 och kan inte \
     ändras i efterhand utan att det syns. De visar att Hemsidor24 uppgett \
     detta, inte att uppgiften är sann.",
    "En signerad rad betyder att Hemsidor24 lämnat ifrån sig kontrollen över \
     artefakten. Den visar inte att kunden tagit emot den, och inte att \
     äganderätt eller upphovsrätt har övergått.",
    "Uppgifter hos domänregistrator, webbhotell eller GitHub kan visa vem som \
     kontrollerar ett konto eller en domän. Vad som gäller rättsligt kan följa \
     av avtal och avgörs inte av detta dokument.",
    "Rader märkta \"studiojournal\" kommer från Hemsidor24:s egen databas. \
     De är daterade och spårbara, men de är inte undertecknade.",
    "Kunden har inte undertecknat något här. Varje post är Hemsidor24:s egen \
     uppgift, inte ett ömsesidigt utbyte.",
    "Tidpunkterna är angivna av Hemsidor24 själv och styrks inte av någon \
     utomstående.",
    "Att nyckeln ovan tillhör Hemsidor24 är en bedömning läsaren gör; det \
     framgår inte av dokumentet i sig.",
];

/// Render the receipt for one order.
///
/// `journal` is the studio's own record, oldest first. `signed` is the handoff
/// history for that customer, as [`crate::Studio::history_for_customer`]
/// returns it.
pub fn render(cell_id: &str, order_id: i64, journal: &[JournalEntry], signed: &[Entry]) -> String {
    let mut out = String::new();

    out.push_str("ÖVERLÄMNINGSKVITTO\n");
    out.push_str("Hemsidor24\n\n");
    out.push_str(&format!("Beställning:  #{order_id}\n"));
    out.push_str(&format!("Utfärdarens nyckel:  {cell_id}\n"));
    out.push_str(
        "Koderna nedan är till för att kontrollera dokumentet. Du behöver dem\n\
         inte för något annat.\n\n",
    );

    out.push_str("VAD SOM HÄNDE\n\n");
    if journal.is_empty() {
        out.push_str("  (inga journalposter)\n\n");
    }
    for entry in journal {
        out.push_str(&format!("  {:<26} {}\n", entry.what, entry.when));
        out.push_str("       Underlag:  studiojournal\n");
        if let Some(promise) = &entry.promise {
            out.push_str(&format!("       Löfte:     {promise}\n"));
        }
        out.push('\n');
    }

    out.push_str("ÖVERLÄMNING\n\n");
    if signed.is_empty() {
        out.push_str("  Inget överlämnande är undertecknat för denna beställning.\n\n");
    }
    for (_, claim, body) in signed {
        let what = match asset_of(&body.item) {
            Some(asset) => asset.label_sv(),
            None => "Överlämnat",
        };
        // Says what the studio did, never what the customer did. `Discharged`
        // upstream is only "the item leaves this chain of accountability" — a
        // closing of the studio's own record, not a discharge of liability.
        let verb = if body.event.to_string() == "discharged" {
            "Studion avslutade sin journal"
        } else {
            "Studion lämnade ifrån sig kontrollen"
        };
        out.push_str(&format!("  {what:<26} {verb}\n"));
        out.push_str(&format!(
            "       Avser:     {}\n",
            identifier_of(&body.item)
        ));
        out.push_str(&format!(
            "       Tid:       {}\n",
            format_timestamp(claim.claim.timestamp_ms)
        ));
        out.push_str("       Underlag:  signerat av Hemsidor24\n");
        out.push_str(&format!("       Verifieringskod: {}\n", claim.id));
        if !body.note.is_empty() {
            out.push_str(&format!("       Notering:  {}\n", body.note));
        }
        out.push('\n');
    }

    out.push_str("VAD SOM UTLOVADES\n\n");
    out.push_str("  Domän och webbhotell i ditt namn. All kod på GitHub — du äger den.\n");
    out.push_str("  Detta är vad som utlovades. Vad detta dokument visar står nedan.\n\n");

    out.push_str("VAD DETTA INTE VISAR\n\n");
    for caveat in CAVEATS_SV {
        out.push_str("  - ");
        out.push_str(&wrap(caveat, 72, "    "));
        out.push('\n');
    }

    out
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
    fn an_empty_receipt_still_says_what_is_missing() {
        let text = render("cell-abc", 42, &[], &[]);
        assert!(text.contains("Beställning:  #42"));
        assert!(text.contains("inga journalposter"));
        assert!(text.contains("Inget överlämnande är undertecknat"));
    }

    #[test]
    fn journal_lines_are_labelled_as_unsigned() {
        let journal = vec![
            JournalEntry::new("2026-09-04", "Förslag visat")
                .settling("Du ser förslaget innan sidan publiceras."),
        ];
        let text = render("cell-abc", 1, &journal, &[]);
        assert!(text.contains("Förslag visat"));
        assert!(text.contains("Underlag:  studiojournal"));
        assert!(text.contains("Du ser förslaget innan sidan publiceras."));
    }

    #[test]
    fn the_two_evidence_tiers_are_both_explained() {
        let text = render("cell-abc", 1, &[], &[]);
        assert!(text.contains("signerat"));
        assert!(text.contains("studiojournal"));
        assert!(text.contains("Kunden har inte undertecknat något här"));
    }

    #[test]
    fn no_protocol_vocabulary_reaches_the_reader() {
        // Canonical identifiers (`cell:…`, `claim:…`) DO appear: without them
        // the document cannot be checked by anyone. What must not appear is
        // the protocol's *concepts* — the reader is never asked to know what a
        // dialect, a chain or a canonical encoding is. So this checks the
        // words around the values, using a realistic identifier.
        let journal = vec![JournalEntry::new("2026-09-04", "Förslag visat")];
        let text = render(&format!("cell:{}", "ab".repeat(32)), 1, &journal, &[]).to_lowercase();
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
        // And the labels a reader actually reads are plain Swedish.
        assert!(text.contains("utfärdarens nyckel"));
        assert!(text.contains("verifieringskod") || text.contains("koderna"));
    }

    #[test]
    fn lines_stay_readable_on_paper() {
        let journal = vec![JournalEntry::new("2026-09-04", "Förslag godkänt")];
        for line in render("cell-abc", 1, &journal, &[]).lines() {
            assert!(line.chars().count() <= 90, "line too long: {line:?}");
        }
    }
}
