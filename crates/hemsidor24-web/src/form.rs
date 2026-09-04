//! The order form as the template sees it.
//!
//! Two jobs: carry what the customer typed back onto the page when something
//! is wrong, and carry the Swedish wording for each error. The rules live in
//! `hemsidor24-core`; the sentences live here, because wording is presentation
//! and the core crate has no opinion about language.

use hemsidor24_core::{Field, FieldError, OrderForm, Package, Style, ValidationErrors};
use serde::Deserialize;

use crate::spam::now_unix;

/// What the browser posts to `/bestall`.
///
/// Field names are the ones already in the HTML, unchanged since the static
/// page. `webbplats` is the honeypot; `oppnad` is when the form was rendered.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct OrderSubmission {
    /// Company name.
    #[serde(default)]
    pub foretag: String,
    /// City.
    #[serde(default)]
    pub ort: String,
    /// Phone number.
    #[serde(default)]
    pub tel: String,
    /// Email address.
    #[serde(default)]
    pub epost: String,
    /// Services, one per line.
    #[serde(default)]
    pub tjanster: String,
    /// Chosen style.
    #[serde(default)]
    pub stil: String,
    /// Chosen package.
    #[serde(default)]
    pub paket: String,
    /// Honeypot. A human never sees this, so it must arrive empty.
    #[serde(default)]
    pub webbplats: String,
    /// Unix seconds when the form was rendered.
    #[serde(default)]
    pub oppnad: String,
}

impl OrderSubmission {
    /// Translate into the core type for validation.
    pub fn to_core(&self) -> OrderForm {
        OrderForm {
            company: self.foretag.clone(),
            city: self.ort.clone(),
            phone: self.tel.clone(),
            email: self.epost.clone(),
            services: self.tjanster.clone(),
            style: self.stil.clone(),
            package: self.paket.clone(),
        }
    }
}

/// One text field: what was typed, and what is wrong with it.
#[derive(Debug, Clone, Default)]
pub struct FieldView {
    /// The value to put back in the input.
    pub value: String,
    /// The Swedish sentence to show under it, if any.
    pub error: Option<String>,
}

/// One radio button.
#[derive(Debug, Clone)]
pub struct ChoiceView {
    /// The `value` attribute, and what is posted back.
    pub value: String,
    /// The label.
    pub name: String,
    /// The line under the label.
    pub desc: String,
    /// Whether it is selected.
    pub checked: bool,
    /// Border colour for the selected state.
    pub border: &'static str,
    /// Background colour for the selected state.
    pub bg: &'static str,
}

impl ChoiceView {
    /// Build a choice, colouring it the way the original page did.
    fn new(value: &str, desc: &str, checked: bool) -> Self {
        ChoiceView {
            value: value.to_owned(),
            name: value.to_owned(),
            desc: desc.to_owned(),
            checked,
            border: if checked { "#1C4F9C" } else { "#C3CBD8" },
            bg: if checked { "#EDF2FA" } else { "#fff" },
        }
    }
}

/// Everything the order form needs to render.
#[derive(Debug, Clone)]
pub struct FormView {
    /// Company name.
    pub company: FieldView,
    /// City.
    pub city: FieldView,
    /// Phone number.
    pub phone: FieldView,
    /// Email address.
    pub email: FieldView,
    /// Services textarea.
    pub services: FieldView,
    /// The six style radios.
    pub styles: Vec<ChoiceView>,
    /// The two package radios.
    pub packages: Vec<ChoiceView>,
    /// Problem with the style group, if any.
    pub style_error: Option<String>,
    /// Problem with the package group, if any.
    pub package_error: Option<String>,
    /// Honeypot value, always echoed back empty.
    pub honeypot: String,
    /// Unix seconds stamped into the form when it was rendered.
    pub opened_at: u64,
}

/// The descriptions under each radio, taken from the page as it already reads.
const STYLE_DESCRIPTIONS: [&str; 6] = [
    "Lugn och seriös",
    "Rent och enkelt",
    "Färg och kontrast",
    "Ungt och lätt",
    "För klubbar och föreningar",
    "Vi väljer åt er",
];

/// Package labels exactly as the radios already carry them.
const PACKAGE_LABELS: [(&str, &str); 2] = [
    ("Start — 2 490 kr", "Vi sköter ändringarna åt er"),
    ("Pro — 4 490 kr", "Ni ändrar innehållet själva"),
];

impl Default for FormView {
    /// A blank form, with the first option in each group selected, exactly as
    /// the static page rendered it.
    fn default() -> Self {
        FormView {
            company: FieldView::default(),
            city: FieldView::default(),
            phone: FieldView::default(),
            email: FieldView::default(),
            services: FieldView::default(),
            styles: Style::ALL
                .iter()
                .zip(STYLE_DESCRIPTIONS)
                .enumerate()
                .map(|(i, (s, desc))| ChoiceView::new(s.label_sv(), desc, i == 0))
                .collect(),
            packages: PACKAGE_LABELS
                .iter()
                .enumerate()
                .map(|(i, (label, desc))| ChoiceView::new(label, desc, i == 0))
                .collect(),
            style_error: None,
            package_error: None,
            honeypot: String::new(),
            opened_at: now_unix(),
        }
    }
}

impl FormView {
    /// Rebuild the form from a rejected submission, keeping what was typed and
    /// attaching the Swedish wording for each error.
    pub fn from_rejected(submission: &OrderSubmission, errors: &ValidationErrors) -> Self {
        let field = |f: Field, value: &str| FieldView {
            value: value.to_owned(),
            error: errors.for_field(f).map(message_sv),
        };

        let chosen_style = submission.stil.parse::<Style>().ok();
        let chosen_package = submission.paket.parse::<Package>().ok();

        FormView {
            company: field(Field::Company, &submission.foretag),
            city: field(Field::City, &submission.ort),
            phone: field(Field::Phone, &submission.tel),
            email: field(Field::Email, &submission.epost),
            services: field(Field::Services, &submission.tjanster),
            styles: Style::ALL
                .iter()
                .zip(STYLE_DESCRIPTIONS)
                .enumerate()
                .map(|(i, (s, desc))| {
                    // Nothing chosen yet? Fall back to the first, as before.
                    let checked = chosen_style.map_or(i == 0, |c| c == *s);
                    ChoiceView::new(s.label_sv(), desc, checked)
                })
                .collect(),
            packages: PACKAGE_LABELS
                .iter()
                .zip(Package::ALL)
                .enumerate()
                .map(|(i, ((label, desc), package))| {
                    let checked = chosen_package.map_or(i == 0, |c| c == package);
                    ChoiceView::new(label, desc, checked)
                })
                .collect(),
            style_error: errors.for_field(Field::Style).map(message_sv),
            package_error: errors.for_field(Field::Package).map(message_sv),
            honeypot: String::new(),
            opened_at: now_unix(),
        }
    }
}

/// The Swedish sentence for one validation failure.
///
/// Kept deliberately plain: say what is wrong, not who is at fault.
fn message_sv(error: FieldError) -> String {
    match error {
        FieldError::Missing => "Fyll i det här fältet.".to_owned(),
        FieldError::TooLong { max } => format!("Får vara högst {max} tecken."),
        FieldError::InvalidEmail => "Kontrollera e-postadressen.".to_owned(),
        FieldError::InvalidPhone => "Kontrollera telefonnumret.".to_owned(),
        FieldError::TooManyServices { max, got } => {
            format!("Vi bygger en sida med upp till {max} tjänster. Du har skrivit {got}.")
        }
        FieldError::UnknownChoice => "Välj ett av alternativen.".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hemsidor24_core::validation::limits;

    fn valid() -> OrderSubmission {
        OrderSubmission {
            foretag: "Malmö Bygg AB".into(),
            ort: "Malmö".into(),
            tel: "070-123 45 67".into(),
            epost: "kontakt@malmobygg.se".into(),
            tjanster: "Badrum\nKök".into(),
            stil: "Djärv".into(),
            paket: "Pro — 4 490 kr".into(),
            ..OrderSubmission::default()
        }
    }

    #[test]
    fn a_blank_form_preselects_the_first_option_in_each_group() {
        let view = FormView::default();
        assert!(view.styles[0].checked);
        assert_eq!(view.styles.iter().filter(|c| c.checked).count(), 1);
        assert!(view.packages[0].checked);
        assert_eq!(view.packages.iter().filter(|c| c.checked).count(), 1);
    }

    #[test]
    fn the_radio_labels_match_the_page() {
        let view = FormView::default();
        let names: Vec<_> = view.styles.iter().map(|c| c.value.as_str()).collect();
        assert_eq!(
            names,
            [
                "Klassisk",
                "Modern",
                "Djärv",
                "Student",
                "Förening",
                "Överraska mig"
            ]
        );
        let paks: Vec<_> = view.packages.iter().map(|c| c.value.as_str()).collect();
        assert_eq!(paks, ["Start — 2 490 kr", "Pro — 4 490 kr"]);
    }

    #[test]
    fn every_radio_value_round_trips_through_core() {
        let view = FormView::default();
        for c in &view.styles {
            assert!(c.value.parse::<Style>().is_ok(), "{}", c.value);
        }
        for c in &view.packages {
            assert!(c.value.parse::<Package>().is_ok(), "{}", c.value);
        }
    }

    #[test]
    fn a_rejected_form_keeps_what_the_customer_typed() {
        let mut submission = valid();
        submission.epost = "trasig".into();
        let errors = submission.to_core().validate().expect_err("bad email");
        let view = FormView::from_rejected(&submission, &errors);

        assert_eq!(view.company.value, "Malmö Bygg AB");
        assert_eq!(view.city.value, "Malmö");
        assert_eq!(view.phone.value, "070-123 45 67");
        assert_eq!(view.services.value, "Badrum\nKök");
        assert_eq!(
            view.email.value, "trasig",
            "the bad value must stay so it can be corrected"
        );
    }

    #[test]
    fn a_rejected_form_keeps_the_radio_selections() {
        let mut submission = valid();
        submission.foretag = String::new();
        let errors = submission
            .to_core()
            .validate()
            .expect_err("missing company");
        let view = FormView::from_rejected(&submission, &errors);

        assert!(view.styles.iter().any(|c| c.checked && c.value == "Djärv"));
        assert!(
            view.packages
                .iter()
                .any(|c| c.checked && c.value == "Pro — 4 490 kr")
        );
    }

    #[test]
    fn only_the_broken_field_gets_an_error() {
        let mut submission = valid();
        submission.epost = "trasig".into();
        let errors = submission.to_core().validate().expect_err("bad email");
        let view = FormView::from_rejected(&submission, &errors);

        assert_eq!(
            view.email.error.as_deref(),
            Some("Kontrollera e-postadressen.")
        );
        assert!(view.company.error.is_none());
        assert!(view.city.error.is_none());
        assert!(view.phone.error.is_none());
        assert!(view.services.error.is_none());
    }

    #[test]
    fn every_error_gets_a_swedish_sentence() {
        for error in [
            FieldError::Missing,
            FieldError::TooLong {
                max: limits::COMPANY_MAX,
            },
            FieldError::InvalidEmail,
            FieldError::InvalidPhone,
            FieldError::TooManyServices { max: 6, got: 9 },
            FieldError::UnknownChoice,
        ] {
            let message = message_sv(error);
            assert!(!message.is_empty());
            assert!(
                message.ends_with('.'),
                "{message:?} should read as a sentence"
            );
        }
    }

    #[test]
    fn too_many_services_says_how_many_were_written() {
        let mut submission = valid();
        submission.tjanster = (1..=9)
            .map(|n| format!("Tjänst {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let errors = submission.to_core().validate().expect_err("nine services");
        let view = FormView::from_rejected(&submission, &errors);
        let message = view.services.error.unwrap_or_default();
        assert!(
            message.contains('6') && message.contains('9'),
            "{message:?}"
        );
    }

    #[test]
    fn the_honeypot_is_never_echoed_back() {
        let mut submission = valid();
        submission.webbplats = "http://spam.example".into();
        submission.foretag = String::new();
        let errors = submission
            .to_core()
            .validate()
            .expect_err("missing company");
        assert_eq!(FormView::from_rejected(&submission, &errors).honeypot, "");
    }
}
