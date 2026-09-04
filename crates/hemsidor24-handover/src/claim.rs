//! The handover dialect: what the studio signs when it hands a site over.
//!
//! Five events, one body shape, one dialect. The protocol never looks inside a
//! body — a reader that has never heard of this dialect can still verify the
//! claim, it just cannot render it — so the encoding only has to be
//! deterministic and bounded.
//!
//! # What goes in a body, and what does not
//!
//! Sijill's own guidance is that a body should carry *references* the issuing
//! cell can resolve locally, never names, because a signed body naming a person
//! names them for as long as the record exists. This dialect follows that for
//! people: a customer appears as `order_ref`, the `orders.id` the studio can
//! look up, never as a company name, email or phone number.
//!
//! It deliberately breaks from it for two fields. `domain` and `repo_url` are
//! the *subject* of the promise — "the domain and the source are yours" is the
//! thing being recorded — and a receipt that said only "a domain was
//! transferred" would be worthless to the reader it is written for. They are
//! also already public: a domain is in WHOIS and the repository is on GitHub.
//! That is a deliberate trade, not an oversight.

use sijill_core::{Decoder, Encoder};
use sijill_dialect::{Dialect, DialectError};

/// Longest domain this dialect will sign. RFC 1035's limit.
pub const MAX_DOMAIN_LEN: usize = 253;

/// Longest repository URL this dialect will sign.
pub const MAX_REPO_URL_LEN: usize = 512;

/// Longest free note this dialect will sign.
pub const MAX_NOTE_LEN: usize = 1024;

/// What happened.
///
/// These are the promises the public page makes out loud, and therefore the
/// ones that get disputed later: a proposal was shown, it was accepted, a
/// revision was spent, money went back, ownership moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Event {
    /// A finished proposal was shown to the customer, before publication.
    DeliveryOffered,
    /// The customer accepted it.
    DeliveryAccepted,
    /// A revision was used. One is included in the price.
    RevisionUsed,
    /// Money went back, under the guarantee.
    RefundIssued,
    /// The domain, the hosting and the source passed to the customer.
    OwnershipTransferred,
}

impl Event {
    /// Every event, in the order they occur in a normal job.
    pub const ALL: [Event; 5] = [
        Event::DeliveryOffered,
        Event::DeliveryAccepted,
        Event::RevisionUsed,
        Event::RefundIssued,
        Event::OwnershipTransferred,
    ];

    /// The wire tag. Never renumber these: they are signed and permanent.
    const fn tag(self) -> u8 {
        match self {
            Event::DeliveryOffered => 1,
            Event::DeliveryAccepted => 2,
            Event::RevisionUsed => 3,
            Event::RefundIssued => 4,
            Event::OwnershipTransferred => 5,
        }
    }

    /// Read a tag back.
    const fn from_tag(tag: u8) -> Option<Event> {
        match tag {
            1 => Some(Event::DeliveryOffered),
            2 => Some(Event::DeliveryAccepted),
            3 => Some(Event::RevisionUsed),
            4 => Some(Event::RefundIssued),
            5 => Some(Event::OwnershipTransferred),
            _ => None,
        }
    }

    /// How this event reads in a receipt.
    pub const fn label_sv(self) -> &'static str {
        match self {
            Event::DeliveryOffered => "Förslag visat",
            Event::DeliveryAccepted => "Förslag godkänt",
            Event::RevisionUsed => "Revidering använd",
            Event::RefundIssued => "Återbetalning gjord",
            Event::OwnershipTransferred => "Äganderätt överförd",
        }
    }

    /// The promise this event settles, in the words the page uses.
    pub const fn promise_sv(self) -> &'static str {
        match self {
            Event::DeliveryOffered => "Du ser förslaget innan sidan publiceras.",
            Event::DeliveryAccepted => "Ni betalar först när ni sett sidan.",
            Event::RevisionUsed => "En revidering ingår.",
            Event::RefundIssued => "Gillar du det inte betalar du ingenting.",
            Event::OwnershipTransferred => {
                "Domän och hosting i ditt namn. All kod på GitHub — du äger den."
            }
        }
    }
}

/// One handover claim.
///
/// A single body shape across all five events, with the fields an event does
/// not use left empty. One shape keeps the encoding trivial to verify by eye
/// and means a reader never has to branch before it can decode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandoverClaim {
    /// What happened.
    pub event: Event,
    /// The `orders.id` this concerns. The customer is never named.
    pub order_ref: u64,
    /// The `sites.id`, or 0 before a site exists.
    pub site_ref: u64,
    /// The domain handed over. Only [`Event::OwnershipTransferred`] sets it.
    pub domain: String,
    /// The repository handed over. Only [`Event::OwnershipTransferred`] sets it.
    pub repo_url: String,
    /// Which revision was spent. 1 is the one included in the price.
    pub revision: u32,
    /// Amount refunded, in öre. Öre so it stays exact.
    pub refund_ore: u64,
    /// Anything a reader would need that the fields above do not carry.
    pub note: String,
}

impl HandoverClaim {
    /// A claim with everything empty but the event and the order it concerns.
    pub fn new(event: Event, order_ref: u64) -> Self {
        HandoverClaim {
            event,
            order_ref,
            site_ref: 0,
            domain: String::new(),
            repo_url: String::new(),
            revision: 0,
            refund_ore: 0,
            note: String::new(),
        }
    }

    /// Name the site this concerns.
    pub fn for_site(mut self, site_ref: u64) -> Self {
        self.site_ref = site_ref;
        self
    }

    /// Record what was handed over.
    pub fn transferring(mut self, domain: impl Into<String>, repo_url: impl Into<String>) -> Self {
        self.domain = domain.into();
        self.repo_url = repo_url.into();
        self
    }

    /// Record which revision was spent.
    pub fn revision(mut self, revision: u32) -> Self {
        self.revision = revision;
        self
    }

    /// Record how much went back, in öre.
    pub fn refunding_ore(mut self, refund_ore: u64) -> Self {
        self.refund_ore = refund_ore;
        self
    }

    /// Add a note for the reader.
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = note.into();
        self
    }
}

impl Dialect for HandoverClaim {
    const ID: &'static str = "hemsidor24-handover";
    const VERSION: u16 = 1;

    fn encode(&self) -> Result<Vec<u8>, DialectError> {
        if self.domain.len() > MAX_DOMAIN_LEN {
            return Err(DialectError::EntryTooLong {
                len: self.domain.len(),
            });
        }
        if self.repo_url.len() > MAX_REPO_URL_LEN {
            return Err(DialectError::EntryTooLong {
                len: self.repo_url.len(),
            });
        }
        if self.note.len() > MAX_NOTE_LEN {
            return Err(DialectError::NotesTooLong {
                len: self.note.len(),
            });
        }

        // Fixed order, every field always present. Two cells encoding the same
        // claim must produce the same bytes.
        let mut encoder = Encoder::new();
        encoder.u8(self.event.tag());
        encoder.u64(self.order_ref);
        encoder.u64(self.site_ref);
        encoder.string(&self.domain)?;
        encoder.string(&self.repo_url)?;
        encoder.u32(self.revision);
        encoder.u64(self.refund_ore);
        encoder.string(&self.note)?;
        Ok(encoder.finish())
    }

    fn decode(bytes: &[u8]) -> Result<Self, DialectError> {
        let mut decoder = Decoder::new(bytes);
        let tag = decoder.u8()?;
        let event = Event::from_tag(tag).ok_or(DialectError::InvalidValue { field: "event" })?;
        let order_ref = decoder.u64()?;
        let site_ref = decoder.u64()?;
        let domain = decoder.string()?.to_owned();
        let repo_url = decoder.string()?.to_owned();
        let revision = decoder.u32()?;
        let refund_ore = decoder.u64()?;
        let note = decoder.string()?.to_owned();
        // Refuse trailing bytes: a body that decodes but has more after it is
        // not the body that was signed.
        decoder.finish()?;

        Ok(HandoverClaim {
            event,
            order_ref,
            site_ref,
            domain,
            repo_url,
            revision,
            refund_ore,
            note,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full() -> HandoverClaim {
        HandoverClaim::new(Event::OwnershipTransferred, 42)
            .for_site(7)
            .transferring("malmobygg.se", "https://github.com/hemsidor24/malmobygg")
            .revision(1)
            .refunding_ore(311_250)
            .with_note("Överlämnat på plats.")
    }

    #[test]
    fn a_claim_survives_the_round_trip() {
        let claim = full();
        let bytes = claim.encode().expect("encodes");
        assert_eq!(HandoverClaim::decode(&bytes).expect("decodes"), claim);
    }

    #[test]
    fn every_event_round_trips() {
        for event in Event::ALL {
            let claim = HandoverClaim::new(event, 1);
            let bytes = claim.encode().expect("encodes");
            assert_eq!(HandoverClaim::decode(&bytes).expect("decodes").event, event);
        }
    }

    #[test]
    fn encoding_is_deterministic() {
        assert_eq!(full().encode().expect("a"), full().encode().expect("b"));
    }

    #[test]
    fn event_tags_are_stable_and_unique() {
        // These are signed and permanent. Renumbering one silently changes
        // what every past claim says.
        let tags: Vec<u8> = Event::ALL.iter().map(|e| e.tag()).collect();
        assert_eq!(tags, [1, 2, 3, 4, 5]);
        for event in Event::ALL {
            assert_eq!(Event::from_tag(event.tag()), Some(event));
        }
        assert_eq!(Event::from_tag(0), None);
        assert_eq!(Event::from_tag(6), None);
    }

    #[test]
    fn an_unknown_event_tag_is_refused() {
        let mut bytes = full().encode().expect("encodes");
        bytes[0] = 99;
        assert!(matches!(
            HandoverClaim::decode(&bytes),
            Err(DialectError::InvalidValue { field: "event" })
        ));
    }

    #[test]
    fn trailing_bytes_are_refused() {
        let mut bytes = full().encode().expect("encodes");
        bytes.push(0);
        assert!(HandoverClaim::decode(&bytes).is_err());
    }

    #[test]
    fn an_over_long_domain_is_refused_before_it_is_signed() {
        let claim = HandoverClaim::new(Event::OwnershipTransferred, 1)
            .transferring("x".repeat(MAX_DOMAIN_LEN + 1), "");
        assert!(matches!(
            claim.encode(),
            Err(DialectError::EntryTooLong { .. })
        ));
    }

    #[test]
    fn an_over_long_note_is_refused_before_it_is_signed() {
        let claim =
            HandoverClaim::new(Event::DeliveryOffered, 1).with_note("x".repeat(MAX_NOTE_LEN + 1));
        assert!(matches!(
            claim.encode(),
            Err(DialectError::NotesTooLong { .. })
        ));
    }

    #[test]
    fn no_customer_name_can_reach_a_body() {
        // The doctrine, enforced by shape: there is nowhere to put one. Every
        // field is either a numeric reference, a public artefact, or a note the
        // studio writes deliberately.
        let claim = full();
        let bytes = claim.encode().expect("encodes");
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains("Malmö Bygg"));
        assert!(!text.contains("kontakt@"));
    }
}
