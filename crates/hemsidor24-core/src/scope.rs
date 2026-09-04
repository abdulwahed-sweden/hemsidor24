//! The scope limits of the product.
//!
//! These are not tuning knobs. Each one is a promise made in the public copy
//! ("Fler än en sida", "Fler än sex tjänster", "En revidering ingår"), and the
//! fixed price and the next-day delivery both depend on them holding.

/// Most services a customer may list on their page.
pub const MAX_SERVICES: usize = 6;

/// Pages in a delivered site. One, by definition of the product.
pub const MAX_PAGES: u8 = 1;

/// Revisions included in the price before further work is billable.
pub const INCLUDED_REVISIONS: u8 = 1;

/// Swedish VAT rate on these services, in percent.
pub const VAT_PERCENT: u32 = 25;
