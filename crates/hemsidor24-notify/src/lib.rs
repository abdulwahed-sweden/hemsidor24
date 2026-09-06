//! Outbound email for Hemsidor24.
//!
//! Two messages go out when an order arrives: one to the studio so the work can
//! start, and one to the customer so they know the form did something. Both are
//! plain text. Nobody reads a marketing email from a company they just paid.
//!
//! Everything is behind [`Notifier`] so the web crate can be tested without an
//! SMTP server anywhere in sight — see [`FakeNotifier`].
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod message;
pub mod smtp;

use std::sync::Mutex;

pub use message::{
    Message, customer_confirmation, order_cancelled, proposal_accepted, proposal_ready,
    refund_issued, site_published, studio_notification,
};
pub use smtp::{SmtpConfig, SmtpNotifier};
use thiserror::Error;

/// Something that can deliver an email.
///
/// Deliberately narrow. The caller decides what to send; this only sends it.
pub trait Notifier: Send + Sync {
    /// Deliver one message.
    ///
    /// # Errors
    ///
    /// Returns [`NotifyError`] if the message could not be handed to the mail
    /// server. A caller must treat this as recoverable: the order is already
    /// saved by the time this is called.
    fn send(
        &self,
        message: &Message,
    ) -> impl std::future::Future<Output = Result<(), NotifyError>> + Send;
}

/// Why a message could not be delivered.
#[derive(Debug, Error)]
pub enum NotifyError {
    /// The address in the configuration or the order could not be parsed.
    #[error("invalid email address {address:?}")]
    Address {
        /// The address that could not be parsed.
        address: String,
        /// The underlying parse failure.
        #[source]
        source: lettre::address::AddressError,
    },
    /// The message could not be assembled.
    #[error("could not build the message")]
    Build(#[source] lettre::error::Error),
    /// The mail server refused it, or could not be reached.
    #[error("could not deliver to the mail server")]
    Transport(#[source] lettre::transport::smtp::Error),
    /// Only ever produced by [`FakeNotifier::failing`], to stand in for a mail
    /// outage in tests.
    #[error("simulated delivery failure")]
    Simulated,
}

/// A notifier that records messages instead of sending them.
///
/// For tests, and for running the site locally without SMTP credentials.
#[derive(Debug, Default)]
pub struct FakeNotifier {
    sent: Mutex<Vec<Message>>,
    /// When true, every [`Notifier::send`] fails. Used to prove an order still
    /// survives a mail outage.
    pub fail: bool,
}

impl FakeNotifier {
    /// A notifier that accepts everything.
    pub fn new() -> Self {
        Self::default()
    }

    /// A notifier that refuses everything.
    pub fn failing() -> Self {
        FakeNotifier {
            sent: Mutex::new(Vec::new()),
            fail: true,
        }
    }

    /// Everything sent so far.
    ///
    /// Returns empty if the lock was poisoned, which can only happen if another
    /// test panicked while holding it.
    pub fn sent(&self) -> Vec<Message> {
        self.sent.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

impl Notifier for FakeNotifier {
    async fn send(&self, message: &Message) -> Result<(), NotifyError> {
        if self.fail {
            return Err(NotifyError::Simulated);
        }
        // Print the whole message, not a summary of it.
        //
        // This is the no-SMTP path and nothing else: when SMTP is configured,
        // SmtpNotifier does the sending and this type is never reached. So the
        // only reader of this output is somebody running without a mail server
        // — locally, or a studio whose settings are missing — and for them a
        // subject line alone answers none of the questions they have. What did
        // the customer actually receive, and does it read correctly in Swedish?
        tracing::info!(
            "NO SMTP — message not sent, printed instead:\n\
             ----------------------------------------------------------------\n\
             To:      {}\n\
             From:    {}\n\
             ReplyTo: {}\n\
             Subject: {}\n\
             \n\
             {}\
             ----------------------------------------------------------------",
            message.to,
            message.from,
            message.reply_to.as_deref().unwrap_or("—"),
            message.subject,
            message.body,
        );

        if let Ok(mut sent) = self.sent.lock() {
            sent.push(message.clone());
        }
        Ok(())
    }
}
