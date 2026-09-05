//! The artefacts the studio hands control of to the customer.
//!
//! The public page promises three of them: a domain, a hosting account and a
//! repository. Each moves separately — a domain can already be in the
//! customer's name while the repository handover is still pending — so each
//! gets its own claim rather than one lumped event, which also matches the
//! three booleans the `deliveries` table already keeps.
//!
//! What a claim about one of these records is narrow, and the wording here is
//! kept narrow to match: the studio asserts it **released control** of the
//! artefact. Whether legal title or copyright moved is a matter for the
//! contract and for the registrar's, host's or forge's own records, and no
//! claim in this crate speaks to it.

use sijill_dialect_handoff::MAX_REF_LEN;

/// One of the three artefacts the studio promises to hand over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Asset {
    /// The domain, registered in the customer's name.
    Domain,
    /// The hosting account, in the customer's name.
    Hosting,
    /// The repository the customer owns.
    SourceCode,
}

impl Asset {
    /// Every asset, in the order the promise names them.
    pub const ALL: [Asset; 3] = [Asset::Domain, Asset::Hosting, Asset::SourceCode];

    /// How it reads on a receipt.
    pub const fn label_sv(self) -> &'static str {
        match self {
            Asset::Domain => "Domän",
            Asset::Hosting => "Webbhotell",
            Asset::SourceCode => "Källkod",
        }
    }

    /// The prefix on this asset's item reference, so a reader can tell the
    /// three apart without resolving them.
    pub const fn prefix(self) -> &'static str {
        match self {
            Asset::Domain => "dom",
            Asset::Hosting => "host",
            Asset::SourceCode => "src",
        }
    }
}

/// One artefact, as the claim will carry it.
///
/// # Why the item is published and the customer is not
///
/// `sijill-dialect-handoff` fixes the *shape* of a reference and leaves the
/// publish / commit / omit choice to the application, endorsing none. This
/// crate makes that choice deliberately, and differently for the two slots:
///
/// **The item is published.** A domain name and a repository URL are the
/// subject of the promise, and they are already public — the domain is in
/// WHOIS, the repository is on GitHub, the site is live. A claim that said
/// only "an item was transferred" would be worthless to the reader this record
/// is written for, and committing a value that anyone can guess from the live
/// site buys nothing.
///
/// **The customer is not.** A signed body naming a company names it for as
/// long as the record exists, and the record is built to be long-lived and
/// widely copied. The counterparty is therefore a local code — `cus-42`, the
/// `customers.id` — resolvable only through records the studio holds. Anyone
/// holding the log learns that *a* customer received a domain, which is
/// already obvious from the domain, and no more.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artefact {
    /// Which of the three promises this artefact belongs to.
    pub asset: Asset,
    /// The artefact itself: the domain name, the hosting account reference, or
    /// the repository URL.
    pub identifier: String,
}

impl Artefact {
    /// Name an artefact whose control is being released.
    pub fn new(asset: Asset, identifier: impl Into<String>) -> Self {
        Artefact {
            asset,
            identifier: identifier.into(),
        }
    }

    /// The item reference as the claim carries it: `dom:malmobygg.se`.
    ///
    /// Prefixed so that three claims about one order are distinguishable
    /// without resolving anything, and so a bare domain is never mistaken for
    /// a repository.
    pub fn item_ref(&self) -> String {
        format!("{}:{}", self.asset.prefix(), self.identifier)
    }

    /// Whether the reference will fit the dialect's bound.
    pub fn fits(&self) -> bool {
        self.item_ref().len() <= MAX_REF_LEN
    }
}

/// The customer, as the claim carries them: `cus-42`.
///
/// Takes the `customers.id` rather than anything the customer typed, so no
/// name, address or email can reach a signed body by accident.
pub fn customer_ref(customer_id: i64) -> String {
    format!("cus-{customer_id}")
}

/// The order, as the claim carries it: `ord-42`.
pub fn order_ref(order_id: i64) -> String {
    format!("ord-{order_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_are_exactly_three_promised_assets() {
        assert_eq!(Asset::ALL.len(), 3);
    }

    #[test]
    fn item_references_are_prefixed_and_distinguishable() {
        let dom = Artefact::new(Asset::Domain, "malmobygg.se");
        let src = Artefact::new(Asset::SourceCode, "https://github.com/h24/malmobygg");
        assert_eq!(dom.item_ref(), "dom:malmobygg.se");
        assert!(src.item_ref().starts_with("src:"));
        assert_ne!(dom.item_ref(), src.item_ref());
    }

    #[test]
    fn prefixes_are_unique() {
        let mut seen: Vec<&str> = Asset::ALL.iter().map(|a| a.prefix()).collect();
        seen.sort_unstable();
        let before = seen.len();
        seen.dedup();
        assert_eq!(seen.len(), before);
    }

    #[test]
    fn realistic_references_fit_the_dialect_bound() {
        assert!(Artefact::new(Asset::Domain, "malmobygg.se").fits());
        assert!(Artefact::new(Asset::SourceCode, "https://github.com/hemsidor24/malmobygg").fits());
        assert!(Artefact::new(Asset::Hosting, "loopia:12345").fits());
    }

    #[test]
    fn an_absurd_reference_is_reported_rather_than_signed() {
        assert!(!Artefact::new(Asset::Domain, "x".repeat(MAX_REF_LEN)).fits());
    }

    #[test]
    fn the_customer_reference_carries_no_identity() {
        let r = customer_ref(42);
        assert_eq!(r, "cus-42");
        // Nothing the customer typed can reach it: the input is a row id.
        assert!(!r.contains('@') && !r.contains(' '));
    }
}
