//! SMTP delivery over STARTTLS.
//!
//! Built for Gmail with an app password on port 587, which is what the studio
//! uses. Nothing here is Gmail-specific beyond that default.

use lettre::message::{Mailbox, MessageBuilder};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};

use crate::{Message, Notifier, NotifyError};

/// Port 587, submission with STARTTLS.
const DEFAULT_SMTP_PORT: u16 = 587;

/// Everything needed to talk to the mail server.
///
/// Read from the environment by the caller. Nothing in this struct belongs in
/// the repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmtpConfig {
    /// Mail server hostname, e.g. `smtp.gmail.com`.
    pub host: String,
    /// Submission port. 587 unless told otherwise.
    pub port: u16,
    /// Username to authenticate with.
    pub user: String,
    /// Password, or a Gmail app password.
    pub password: String,
    /// Where studio notifications go.
    pub notify_to: String,
    /// Envelope sender for both messages.
    pub notify_from: String,
}

impl SmtpConfig {
    /// Port 587 unless the caller says otherwise.
    pub const DEFAULT_PORT: u16 = DEFAULT_SMTP_PORT;
}

/// Sends mail over SMTP with STARTTLS.
pub struct SmtpNotifier {
    transport: AsyncSmtpTransport<Tokio1Executor>,
}

impl std::fmt::Debug for SmtpNotifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never derive Debug here: the transport holds the password.
        f.debug_struct("SmtpNotifier").finish_non_exhaustive()
    }
}

impl SmtpNotifier {
    /// Connect lazily to the configured mail server.
    ///
    /// `starttls_relay` upgrades a plaintext connection on 587, which is what
    /// Gmail expects. No mail is sent until [`Notifier::send`] is called.
    ///
    /// # Errors
    ///
    /// Returns [`NotifyError::Transport`] if the host is unusable.
    pub fn connect(config: &SmtpConfig) -> Result<Self, NotifyError> {
        let credentials = Credentials::new(config.user.clone(), config.password.clone());

        let transport = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host)
            .map_err(NotifyError::Transport)?
            .port(config.port)
            .credentials(credentials)
            .build();

        Ok(SmtpNotifier { transport })
    }
}

/// Parse an address, naming it in the error if it will not parse.
fn mailbox(address: &str) -> Result<Mailbox, NotifyError> {
    address
        .parse::<Mailbox>()
        .map_err(|source| NotifyError::Address {
            address: address.to_owned(),
            source,
        })
}

impl Notifier for SmtpNotifier {
    async fn send(&self, message: &Message) -> Result<(), NotifyError> {
        let mut builder: MessageBuilder = lettre::Message::builder()
            .from(mailbox(&message.from)?)
            .to(mailbox(&message.to)?)
            .subject(&message.subject);

        if let Some(reply_to) = &message.reply_to {
            builder = builder.reply_to(mailbox(reply_to)?);
        }

        let email = builder
            .header(lettre::message::header::ContentType::TEXT_PLAIN)
            .body(message.body.clone())
            .map_err(NotifyError::Build)?;

        self.transport
            .send(email)
            .await
            .map_err(NotifyError::Transport)?;
        Ok(())
    }
}
