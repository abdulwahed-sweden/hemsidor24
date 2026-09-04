//! The two packages a customer can order.

use crate::scope::VAT_PERCENT;
use crate::validation::ParseChoiceError;
use std::str::FromStr;

/// A package. There are two, and adding a third is a product decision, not a
/// code change made in passing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Package {
    /// Studio makes content changes on the customer's behalf.
    Start,
    /// Customer gets a login and edits the content themselves.
    Pro,
}

impl Package {
    /// Every package, in the order they are shown on the page.
    pub const ALL: [Package; 2] = [Package::Start, Package::Pro];

    /// Stable identifier used in URLs, form values and the database.
    ///
    /// Unlike [`Package::label_sv`] this must never change: it is written to
    /// Postgres from phase 3 on.
    pub const fn slug(self) -> &'static str {
        match self {
            Package::Start => "start",
            Package::Pro => "pro",
        }
    }

    /// Name as shown to the customer.
    pub const fn label_sv(self) -> &'static str {
        match self {
            Package::Start => "Start",
            Package::Pro => "Pro",
        }
    }

    /// Price in whole kronor, excluding VAT.
    pub const fn price_sek_ex_vat(self) -> u32 {
        match self {
            Package::Start => 2_490,
            Package::Pro => 4_490,
        }
    }

    /// Price including VAT, in öre.
    ///
    /// Öre rather than kronor because both prices land on half a krona and
    /// this stays exact. Floats have no business anywhere near a price.
    pub const fn price_ore_inc_vat(self) -> u32 {
        self.price_sek_ex_vat() * 100 * (100 + VAT_PERCENT) / 100
    }

    /// Whether the customer can edit their own content after delivery.
    pub const fn customer_edits_own_content(self) -> bool {
        matches!(self, Package::Pro)
    }
}

impl FromStr for Package {
    type Err = ParseChoiceError;

    /// Accepts the slug, and the Swedish label as the order form submits it.
    ///
    /// The form posts the full radio label (`"Start — 2 490 kr"`), so the
    /// prefix match keeps the parser working if the price in the copy changes
    /// without anyone remembering this file exists.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let t = s.trim();
        Package::ALL
            .into_iter()
            .find(|p| {
                t.eq_ignore_ascii_case(p.slug())
                    || t.eq_ignore_ascii_case(p.label_sv())
                    || t.starts_with(p.label_sv())
            })
            .ok_or(ParseChoiceError::Package)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prices_match_the_public_copy() {
        assert_eq!(Package::Start.price_sek_ex_vat(), 2_490);
        assert_eq!(Package::Pro.price_sek_ex_vat(), 4_490);
    }

    #[test]
    fn vat_inclusive_prices_match_the_public_copy() {
        // The page prints "3 112,50 kr inkl. moms" and "5 612,50 kr inkl. moms".
        assert_eq!(Package::Start.price_ore_inc_vat(), 311_250);
        assert_eq!(Package::Pro.price_ore_inc_vat(), 561_250);
    }

    #[test]
    fn parses_slug_and_form_label() {
        assert_eq!("start".parse(), Ok(Package::Start));
        assert_eq!("Pro".parse(), Ok(Package::Pro));
        assert_eq!("Start — 2 490 kr".parse(), Ok(Package::Start));
        assert_eq!("Pro — 4 490 kr".parse(), Ok(Package::Pro));
    }

    #[test]
    fn rejects_unknown_package() {
        assert_eq!(
            "Enterprise".parse::<Package>(),
            Err(ParseChoiceError::Package)
        );
        assert_eq!("".parse::<Package>(), Err(ParseChoiceError::Package));
    }

    #[test]
    fn slugs_are_unique() {
        assert_ne!(Package::Start.slug(), Package::Pro.slug());
    }

    #[test]
    fn only_pro_is_self_service() {
        assert!(!Package::Start.customer_edits_own_content());
        assert!(Package::Pro.customer_edits_own_content());
    }
}
