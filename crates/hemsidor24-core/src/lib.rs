//! Domain types and validation rules for Hemsidor24.
//!
//! This crate is deliberately inert: no I/O, no web framework, no database, no
//! clock. Everything here is a value or a rule about a value, so it can be
//! tested without spinning anything up. Timestamps, IP hashes and user agents
//! are persistence concerns and live in the web crate from phase 3 on.
//!
//! The rules encoded here are the ones the sales copy promises out loud —
//! one page, at most six services, one revision included. See [`scope`].
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod order;
pub mod package;
pub mod scope;
pub mod status;
pub mod style;
pub mod validation;

pub use order::{Order, OrderForm, Services};
pub use package::Package;
pub use status::{OrderStatus, TransitionError};
pub use style::Style;
pub use validation::{Field, FieldError, ParseChoiceError, ValidationErrors};
