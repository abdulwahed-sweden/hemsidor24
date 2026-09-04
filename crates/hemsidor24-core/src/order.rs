//! The order itself: what the customer submits, and what a valid one becomes.
//!
//! Two types on purpose. [`OrderForm`] is whatever arrived over the wire —
//! always constructible, never trusted. [`Order`] can only be produced by
//! [`OrderForm::validate`], so holding one is proof the rules were applied.
//! Phase 3 needs both: the valid order to store, the raw form to re-render
//! with the customer's own text still in the boxes.

use crate::package::Package;
use crate::scope::MAX_SERVICES;
use crate::style::Style;
use crate::validation::{Field, FieldError, ValidationErrors, limits};

/// A raw submission. Every field is a string because that is what an HTML form
/// gives you, and none of them are trusted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OrderForm {
    /// Company name, `foretag`.
    pub company: String,
    /// City, `ort`.
    pub city: String,
    /// Phone number, `tel`.
    pub phone: String,
    /// Email address, `epost`.
    pub email: String,
    /// The services textarea, `tjanster`. One service per line.
    pub services: String,
    /// The selected style, `stil`.
    pub style: String,
    /// The selected package, `paket`.
    pub package: String,
}

impl OrderForm {
    /// Check every field and either build an [`Order`] or report everything
    /// that is wrong at once.
    pub fn validate(&self) -> Result<Order, ValidationErrors> {
        let mut errors = ValidationErrors::new();

        let company = errors.collect(
            Field::Company,
            required_text(&self.company, limits::COMPANY_MAX),
        );
        let city = errors.collect(Field::City, required_text(&self.city, limits::CITY_MAX));
        let phone = errors.collect(Field::Phone, validate_phone(&self.phone));
        let email = errors.collect(Field::Email, validate_email(&self.email));
        let services = errors.collect(Field::Services, Services::parse(&self.services));
        let style = errors.collect(
            Field::Style,
            self.style
                .parse::<Style>()
                .map_err(|_| FieldError::UnknownChoice),
        );
        let package = errors.collect(
            Field::Package,
            self.package
                .parse::<Package>()
                .map_err(|_| FieldError::UnknownChoice),
        );

        // Every `Some` here is guaranteed by `errors` being empty, but the
        // types do not know that, so unpack rather than unwrap.
        match (company, city, phone, email, services, style, package) {
            (
                Some(company),
                Some(city),
                Some(phone),
                Some(email),
                Some(services),
                Some(style),
                Some(package),
            ) if errors.is_empty() => Ok(Order {
                company,
                city,
                phone,
                email,
                services,
                style,
                package,
            }),
            _ => Err(errors),
        }
    }
}

/// A validated order.
///
/// Fields are private: the only way to get one is [`OrderForm::validate`], and
/// that is the point. No timestamp, no id, no IP — those are added by the
/// storage layer, which knows what time it is. This type does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Order {
    company: String,
    city: String,
    phone: String,
    email: String,
    services: Services,
    style: Style,
    package: Package,
}

impl Order {
    /// Company name, trimmed and non-empty.
    pub fn company(&self) -> &str {
        &self.company
    }

    /// City, trimmed and non-empty.
    pub fn city(&self) -> &str {
        &self.city
    }

    /// Phone number as the customer wrote it, trimmed.
    pub fn phone(&self) -> &str {
        &self.phone
    }

    /// Email address, trimmed.
    pub fn email(&self) -> &str {
        &self.email
    }

    /// The services, between one and [`MAX_SERVICES`] of them.
    pub fn services(&self) -> &Services {
        &self.services
    }

    /// The chosen style.
    pub fn style(&self) -> Style {
        self.style
    }

    /// The chosen package.
    pub fn package(&self) -> Package {
        self.package
    }
}

/// The services a customer listed: at least one, never more than
/// [`MAX_SERVICES`], each trimmed and non-empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Services(Vec<String>);

impl Services {
    /// Split a textarea into services, one per line.
    ///
    /// Blank lines are dropped rather than rejected — people pad lists with
    /// them, and refusing to accept an order over an empty line would be
    /// exactly the kind of annoyance this product is supposed to avoid.
    pub fn parse(raw: &str) -> Result<Self, FieldError> {
        let lines: Vec<String> = raw
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect();

        if lines.is_empty() {
            return Err(FieldError::Missing);
        }
        if lines.len() > MAX_SERVICES {
            return Err(FieldError::TooManyServices {
                max: MAX_SERVICES,
                got: lines.len(),
            });
        }
        if lines
            .iter()
            .any(|line| line.chars().count() > limits::SERVICE_MAX)
        {
            return Err(FieldError::TooLong {
                max: limits::SERVICE_MAX,
            });
        }
        Ok(Services(lines))
    }

    /// The services, in the order the customer wrote them.
    pub fn as_slice(&self) -> &[String] {
        &self.0
    }

    /// How many services were listed. Always between 1 and [`MAX_SERVICES`].
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Always `false`; a `Services` cannot be empty. Present because clippy
    /// asks for it wherever there is a `len`.
    pub fn is_empty(&self) -> bool {
        false
    }
}

/// Trim, reject empty, reject over-long.
fn required_text(raw: &str, max: usize) -> Result<String, FieldError> {
    let t = raw.trim();
    if t.is_empty() {
        Err(FieldError::Missing)
    } else if t.chars().count() > max {
        Err(FieldError::TooLong { max })
    } else {
        Ok(t.to_owned())
    }
}

/// Deliberately loose. A Swedish small business might write `070-123 45 67`,
/// `+46 70 123 45 67` or `0701234567`, and none of those should be argued
/// with. We check there are enough digits and nothing alarming, and leave the
/// formatting alone.
fn validate_phone(raw: &str) -> Result<String, FieldError> {
    let t = raw.trim();
    if t.is_empty() {
        return Err(FieldError::Missing);
    }
    if t.chars().count() > limits::PHONE_MAX {
        return Err(FieldError::TooLong {
            max: limits::PHONE_MAX,
        });
    }
    let digits = t.chars().filter(char::is_ascii_digit).count();
    let shaped_like_a_number = t
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '+' | '-' | ' ' | '(' | ')' | '.' | '/'));
    if digits < limits::PHONE_MIN_DIGITS || !shaped_like_a_number {
        return Err(FieldError::InvalidPhone);
    }
    Ok(t.to_owned())
}

/// Also deliberately loose. The only email address that truly validates is one
/// that receives mail, and the confirmation mail in phase 3 is the real check.
/// This rejects the typos worth catching before the customer walks away.
fn validate_email(raw: &str) -> Result<String, FieldError> {
    let t = raw.trim();
    if t.is_empty() {
        return Err(FieldError::Missing);
    }
    if t.chars().count() > limits::EMAIL_MAX {
        return Err(FieldError::TooLong {
            max: limits::EMAIL_MAX,
        });
    }
    let mut parts = t.split('@');
    let valid = match (parts.next(), parts.next(), parts.next()) {
        (Some(local), Some(domain), None) => {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
                && !domain.contains("..")
                && !t.chars().any(char::is_whitespace)
        }
        _ => false,
    };
    if valid {
        Ok(t.to_owned())
    } else {
        Err(FieldError::InvalidEmail)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A submission that should always pass, so each test can break one thing.
    fn valid_form() -> OrderForm {
        OrderForm {
            company: "Malmö Bygg AB".into(),
            city: "Malmö".into(),
            phone: "070-123 45 67".into(),
            email: "kontakt@malmobygg.se".into(),
            services: "Badrum 15 000 kr\nKök 25 000 kr\nMålning 500 kr/tim".into(),
            style: "Klassisk".into(),
            package: "Start — 2 490 kr".into(),
        }
    }

    #[test]
    fn a_good_submission_validates() {
        let order = valid_form().validate().expect("this form should be valid");
        assert_eq!(order.company(), "Malmö Bygg AB");
        assert_eq!(order.city(), "Malmö");
        assert_eq!(order.phone(), "070-123 45 67");
        assert_eq!(order.email(), "kontakt@malmobygg.se");
        assert_eq!(order.style(), Style::Klassisk);
        assert_eq!(order.package(), Package::Start);
        assert_eq!(order.services().len(), 3);
    }

    #[test]
    fn text_fields_are_trimmed() {
        let form = OrderForm {
            company: "  Malmö Bygg AB \n".into(),
            ..valid_form()
        };
        let order = form.validate().expect("whitespace should not fail a form");
        assert_eq!(order.company(), "Malmö Bygg AB");
    }

    #[test]
    fn blank_required_fields_are_reported_together() {
        let form = OrderForm {
            company: "   ".into(),
            city: String::new(),
            phone: "\t".into(),
            email: String::new(),
            services: "\n\n".into(),
            ..valid_form()
        };
        let errs = form
            .validate()
            .expect_err("an empty form should not validate");
        assert_eq!(
            errs.len(),
            5,
            "every blank field should be reported at once"
        );
        for field in [
            Field::Company,
            Field::City,
            Field::Phone,
            Field::Email,
            Field::Services,
        ] {
            assert_eq!(
                errs.for_field(field),
                Some(FieldError::Missing),
                "{field:?}"
            );
        }
    }

    #[test]
    fn six_services_are_allowed_and_seven_are_not() {
        let six = (1..=6)
            .map(|n| format!("Tjänst {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let form = OrderForm {
            services: six,
            ..valid_form()
        };
        assert_eq!(
            form.validate().expect("six is the limit").services().len(),
            MAX_SERVICES
        );

        let seven = (1..=7)
            .map(|n| format!("Tjänst {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let form = OrderForm {
            services: seven,
            ..valid_form()
        };
        let errs = form.validate().expect_err("seven services is out of scope");
        assert_eq!(
            errs.for_field(Field::Services),
            Some(FieldError::TooManyServices {
                max: MAX_SERVICES,
                got: 7
            })
        );
    }

    #[test]
    fn blank_lines_between_services_are_forgiven() {
        let form = OrderForm {
            services: "\n Badrum \n\n\n Kök \n".into(),
            ..valid_form()
        };
        let order = form
            .validate()
            .expect("padding lines should not fail a form");
        assert_eq!(order.services().as_slice(), ["Badrum", "Kök"]);
    }

    #[test]
    fn services_keep_the_order_they_were_written_in() {
        let form = OrderForm {
            services: "Först\nSedan\nSist".into(),
            ..valid_form()
        };
        let order = form.validate().expect("valid form");
        assert_eq!(order.services().as_slice(), ["Först", "Sedan", "Sist"]);
    }

    #[test]
    fn an_over_long_service_line_is_rejected() {
        let form = OrderForm {
            services: "x".repeat(limits::SERVICE_MAX + 1),
            ..valid_form()
        };
        let errs = form
            .validate()
            .expect_err("a 201-character service is too long");
        assert_eq!(
            errs.for_field(Field::Services),
            Some(FieldError::TooLong {
                max: limits::SERVICE_MAX
            })
        );
    }

    #[test]
    fn over_long_company_name_is_rejected_by_characters_not_bytes() {
        // 'ö' is two bytes; the limit is in characters, so this must pass.
        let form = OrderForm {
            company: "ö".repeat(limits::COMPANY_MAX),
            ..valid_form()
        };
        assert!(
            form.validate().is_ok(),
            "120 characters should fit even if 240 bytes"
        );

        let form = OrderForm {
            company: "ö".repeat(limits::COMPANY_MAX + 1),
            ..valid_form()
        };
        let errs = form.validate().expect_err("121 characters is too long");
        assert_eq!(
            errs.for_field(Field::Company),
            Some(FieldError::TooLong {
                max: limits::COMPANY_MAX
            })
        );
    }

    #[test]
    fn swedish_phone_formats_are_accepted_as_written() {
        for raw in [
            "070-123 45 67",
            "+46 70 123 45 67",
            "0701234567",
            "(08) 123 456",
            "08/12 34 56",
        ] {
            let form = OrderForm {
                phone: raw.into(),
                ..valid_form()
            };
            let order = form
                .validate()
                .unwrap_or_else(|_| panic!("{raw} should be accepted"));
            assert_eq!(order.phone(), raw, "the number should be stored as written");
        }
    }

    #[test]
    fn nonsense_phone_numbers_are_rejected() {
        for raw in ["ring mig", "12345", "070-123 45 67 <script>"] {
            let form = OrderForm {
                phone: raw.into(),
                ..valid_form()
            };
            let errs = form.validate().unwrap_err();
            assert_eq!(
                errs.for_field(Field::Phone),
                Some(FieldError::InvalidPhone),
                "{raw}"
            );
        }
    }

    #[test]
    fn plausible_emails_are_accepted() {
        for raw in [
            "a@b.se",
            "kontakt@malmobygg.se",
            "first.last+tag@sub.example.co.uk",
        ] {
            let form = OrderForm {
                email: raw.into(),
                ..valid_form()
            };
            assert!(form.validate().is_ok(), "{raw} should be accepted");
        }
    }

    #[test]
    fn broken_emails_are_rejected() {
        for raw in [
            "kontakt",
            "kontakt@",
            "@malmobygg.se",
            "a@b",
            "a@@b.se",
            "a b@c.se",
            "a@.se",
            "a@b..se",
        ] {
            let form = OrderForm {
                email: raw.into(),
                ..valid_form()
            };
            let errs = form.validate().unwrap_err();
            assert_eq!(
                errs.for_field(Field::Email),
                Some(FieldError::InvalidEmail),
                "{raw}"
            );
        }
    }

    #[test]
    fn tampered_radio_values_are_rejected() {
        let form = OrderForm {
            style: "Brutalist".into(),
            package: "Gratis".into(),
            ..valid_form()
        };
        let errs = form
            .validate()
            .expect_err("unknown choices should not validate");
        assert_eq!(
            errs.for_field(Field::Style),
            Some(FieldError::UnknownChoice)
        );
        assert_eq!(
            errs.for_field(Field::Package),
            Some(FieldError::UnknownChoice)
        );
    }

    #[test]
    fn every_offered_choice_validates() {
        for style in Style::ALL {
            for package in Package::ALL {
                let form = OrderForm {
                    style: style.label_sv().into(),
                    package: package.label_sv().into(),
                    ..valid_form()
                };
                let order = form
                    .validate()
                    .unwrap_or_else(|_| panic!("{style:?} + {package:?} is offered on the page"));
                assert_eq!(order.style(), style);
                assert_eq!(order.package(), package);
            }
        }
    }
}
