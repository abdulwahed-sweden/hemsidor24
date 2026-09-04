//! What can be wrong with a submitted order, and where.
//!
//! Errors here are *structured*, not worded. The Swedish sentences the
//! customer reads belong to the templates in the web crate — this crate has no
//! opinion about wording, and the `Display` impls exist for logs.

use thiserror::Error;

/// Length limits on the free-text fields.
///
/// Generous enough that no honest submission hits them, tight enough that the
/// database and the notification email stay sane.
pub mod limits {
    /// Longest accepted company name, in characters.
    pub const COMPANY_MAX: usize = 120;
    /// Longest accepted city name, in characters.
    pub const CITY_MAX: usize = 80;
    /// Longest accepted phone number, in characters.
    pub const PHONE_MAX: usize = 32;
    /// Fewest digits a phone number must contain.
    pub const PHONE_MIN_DIGITS: usize = 6;
    /// Longest accepted email address, in characters. Matches RFC 5321.
    pub const EMAIL_MAX: usize = 254;
    /// Longest accepted single service line, in characters.
    pub const SERVICE_MAX: usize = 200;
}

/// A field on the order form.
///
/// The string form is the HTML `name` attribute, so the web layer can put an
/// error next to the input the customer actually typed into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Field {
    /// `foretag` — company name.
    Company,
    /// `ort` — city.
    City,
    /// `tel` — phone number.
    Phone,
    /// `epost` — email address.
    Email,
    /// `tjanster` — the services textarea.
    Services,
    /// `stil` — the style radio group.
    Style,
    /// `paket` — the package radio group.
    Package,
}

impl Field {
    /// The HTML `name` of the input this field maps to.
    pub const fn form_name(self) -> &'static str {
        match self {
            Field::Company => "foretag",
            Field::City => "ort",
            Field::Phone => "tel",
            Field::Email => "epost",
            Field::Services => "tjanster",
            Field::Style => "stil",
            Field::Package => "paket",
        }
    }
}

/// What is wrong with one field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum FieldError {
    /// Left blank, or nothing but whitespace.
    #[error("required")]
    Missing,
    /// Longer than the field allows.
    #[error("longer than {max} characters")]
    TooLong {
        /// The limit that was exceeded.
        max: usize,
    },
    /// Does not look like an email address.
    #[error("not a valid email address")]
    InvalidEmail,
    /// Does not look like a phone number.
    #[error("not a valid phone number")]
    InvalidPhone,
    /// More service lines than the product covers.
    #[error("{got} services listed, at most {max} are included")]
    TooManyServices {
        /// The scope limit.
        max: usize,
        /// How many the customer listed.
        got: usize,
    },
    /// A radio value that is not one of the offered choices. Normally only
    /// reachable by tampering with the form.
    #[error("not one of the available choices")]
    UnknownChoice,
}

/// Everything wrong with a submission, in field order.
///
/// Collected rather than short-circuited: the customer should see all their
/// mistakes at once, not one per round trip.
#[derive(Debug, Clone, Default, PartialEq, Eq, Error)]
#[error("the order form has {} invalid field(s)", self.len())]
pub struct ValidationErrors {
    errors: Vec<(Field, FieldError)>,
}

impl ValidationErrors {
    /// An empty set.
    pub const fn new() -> Self {
        ValidationErrors { errors: Vec::new() }
    }

    /// Record a problem with a field.
    pub fn push(&mut self, field: Field, error: FieldError) {
        self.errors.push((field, error));
    }

    /// Record a problem only if `result` failed, and hand back the value if it
    /// succeeded.
    pub fn collect<T>(&mut self, field: Field, result: Result<T, FieldError>) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(error) => {
                self.push(field, error);
                None
            }
        }
    }

    /// Whether anything is wrong.
    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    /// How many fields are invalid.
    pub fn len(&self) -> usize {
        self.errors.len()
    }

    /// The problem with one field, if it has one.
    pub fn for_field(&self, field: Field) -> Option<FieldError> {
        self.errors
            .iter()
            .find(|(f, _)| *f == field)
            .map(|(_, e)| *e)
    }

    /// Every problem, in the order the fields were checked.
    pub fn iter(&self) -> impl Iterator<Item = (Field, FieldError)> + '_ {
        self.errors.iter().copied()
    }

    /// Turn into `Ok(value)` when nothing is wrong.
    pub fn into_result<T>(self, value: T) -> Result<T, ValidationErrors> {
        if self.is_empty() {
            Ok(value)
        } else {
            Err(self)
        }
    }
}

/// A radio value that did not name a known [`crate::Style`] or
/// [`crate::Package`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ParseChoiceError {
    /// Not a known style.
    #[error("not a known style")]
    Style,
    /// Not a known package.
    #[error("not a known package")]
    Package,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_names_match_the_existing_html_inputs() {
        assert_eq!(Field::Company.form_name(), "foretag");
        assert_eq!(Field::City.form_name(), "ort");
        assert_eq!(Field::Phone.form_name(), "tel");
        assert_eq!(Field::Email.form_name(), "epost");
        assert_eq!(Field::Services.form_name(), "tjanster");
        assert_eq!(Field::Style.form_name(), "stil");
        assert_eq!(Field::Package.form_name(), "paket");
    }

    #[test]
    fn errors_are_collected_not_short_circuited() {
        let mut errs = ValidationErrors::new();
        errs.push(Field::Company, FieldError::Missing);
        errs.push(Field::Email, FieldError::InvalidEmail);
        assert_eq!(errs.len(), 2);
        assert_eq!(errs.for_field(Field::Company), Some(FieldError::Missing));
        assert_eq!(errs.for_field(Field::Email), Some(FieldError::InvalidEmail));
        assert_eq!(errs.for_field(Field::City), None);
    }

    #[test]
    fn collect_passes_values_through_and_captures_failures() {
        let mut errs = ValidationErrors::new();
        assert_eq!(
            errs.collect(Field::City, Ok::<_, FieldError>("Malmö")),
            Some("Malmö")
        );
        assert!(errs.is_empty());
        assert_eq!(
            errs.collect::<&str>(Field::City, Err(FieldError::Missing)),
            None
        );
        assert_eq!(errs.for_field(Field::City), Some(FieldError::Missing));
    }

    #[test]
    fn into_result_is_ok_only_when_empty() {
        assert_eq!(ValidationErrors::new().into_result(7), Ok(7));
        let mut errs = ValidationErrors::new();
        errs.push(Field::Phone, FieldError::InvalidPhone);
        assert!(errs.into_result(7).is_err());
    }
}
