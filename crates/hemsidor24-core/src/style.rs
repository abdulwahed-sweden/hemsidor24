//! The six visual styles offered on the order form.

use crate::validation::ParseChoiceError;
use std::str::FromStr;

/// A style choice. Six of them, matching the radio buttons on the page.
///
/// This is the customer's *preference*, not a promise of a bespoke design —
/// the copy is explicit that custom layout and custom fonts are out of scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Style {
    /// "Lugn och seriös"
    Klassisk,
    /// "Rent och enkelt"
    Modern,
    /// "Färg och kontrast"
    Djarv,
    /// "Ungt och lätt"
    Student,
    /// "För klubbar och föreningar"
    Forening,
    /// "Vi väljer åt er"
    OverraskaMig,
}

impl Style {
    /// Every style, in the order they are shown on the page.
    pub const ALL: [Style; 6] = [
        Style::Klassisk,
        Style::Modern,
        Style::Djarv,
        Style::Student,
        Style::Forening,
        Style::OverraskaMig,
    ];

    /// Stable identifier used in form values and the database. ASCII only, so
    /// it survives URLs and column values without encoding surprises.
    pub const fn slug(self) -> &'static str {
        match self {
            Style::Klassisk => "klassisk",
            Style::Modern => "modern",
            Style::Djarv => "djarv",
            Style::Student => "student",
            Style::Forening => "forening",
            Style::OverraskaMig => "overraska-mig",
        }
    }

    /// Name as shown to the customer.
    pub const fn label_sv(self) -> &'static str {
        match self {
            Style::Klassisk => "Klassisk",
            Style::Modern => "Modern",
            Style::Djarv => "Djärv",
            Style::Student => "Student",
            Style::Forening => "Förening",
            Style::OverraskaMig => "Överraska mig",
        }
    }

    /// Whether the studio picks the style instead of the customer.
    pub const fn studio_chooses(self) -> bool {
        matches!(self, Style::OverraskaMig)
    }
}

impl FromStr for Style {
    type Err = ParseChoiceError;

    /// Accepts the slug or the Swedish label. The order form posts the label.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let t = s.trim();
        Style::ALL
            .into_iter()
            .find(|st| t.eq_ignore_ascii_case(st.slug()) || t == st.label_sv())
            .ok_or(ParseChoiceError::Style)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_are_exactly_six_styles() {
        assert_eq!(Style::ALL.len(), 6);
    }

    #[test]
    fn labels_match_the_order_form() {
        let labels: Vec<_> = Style::ALL.iter().map(|s| s.label_sv()).collect();
        assert_eq!(
            labels,
            [
                "Klassisk",
                "Modern",
                "Djärv",
                "Student",
                "Förening",
                "Överraska mig"
            ]
        );
    }

    #[test]
    fn slugs_are_unique_and_ascii() {
        let mut slugs: Vec<_> = Style::ALL.iter().map(|s| s.slug()).collect();
        slugs.sort_unstable();
        let count = slugs.len();
        slugs.dedup();
        assert_eq!(slugs.len(), count, "two styles share a slug");
        assert!(slugs.iter().all(|s| s.is_ascii()));
    }

    #[test]
    fn every_label_round_trips_through_parsing() {
        for style in Style::ALL {
            assert_eq!(style.label_sv().parse(), Ok(style));
            assert_eq!(style.slug().parse(), Ok(style));
        }
    }

    #[test]
    fn rejects_unknown_style() {
        assert_eq!("Brutalist".parse::<Style>(), Err(ParseChoiceError::Style));
    }

    #[test]
    fn only_surprise_me_defers_to_the_studio() {
        let deferring: Vec<_> = Style::ALL
            .into_iter()
            .filter(|s| s.studio_chooses())
            .collect();
        assert_eq!(deferring, [Style::OverraskaMig]);
    }
}
