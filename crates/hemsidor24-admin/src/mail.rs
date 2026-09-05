//! The back office's outbound mail.
//!
//! The web crate already reads these five variables and builds the same
//! notifier; the back office needs its own because a bulk action runs with the
//! database and the selected ids and nothing else — there is no application
//! state to reach into. So the notifier is built once at startup and parked
//! here.
//!
//! Same policy as the public site, deliberately: missing mail settings are
//! logged loudly and are not fatal. Refusing to start the back office because
//! SMTP is unreachable would take away the operator's ability to see the
//! orders that are already safe in Postgres.

use std::sync::OnceLock;

use hemsidor24_notify::{FakeNotifier, Message, Notifier, SmtpConfig, SmtpNotifier};

/// Built once at startup by [`init`].
static MAIL: OnceLock<Mail> = OnceLock::new();

/// Where mail goes, and what sends it.
pub struct Mail {
    notifier: Option<SmtpNotifier>,
    /// Kept even when SMTP is absent, so a message that cannot be sent can
    /// still be logged with its intended recipient.
    fallback: FakeNotifier,
    /// The studio's own address — the reply-to a customer sees.
    pub studio: String,
    /// Envelope sender.
    pub from: String,
}

impl Mail {
    /// Send, or log loudly why not.
    ///
    /// Never returns an error. Everything that calls this has already written
    /// its change to Postgres, and a mail failure must not undo that or fail
    /// the operator's action — same rule the public order form follows.
    pub async fn send(&self, message: &Message) {
        let result = match &self.notifier {
            Some(smtp) => smtp.send(message).await,
            None => {
                log::error!(
                    "NO SMTP CONFIGURED — not sending {:?} to {}",
                    message.subject,
                    message.to
                );
                self.fallback.send(message).await
            }
        };
        if let Err(error) = result {
            log::error!(
                "MAIL FAILED — {:?} to {} was not sent: {error}",
                message.subject,
                message.to
            );
        }
    }
}

/// Read the mail settings and build the notifier. Call once, at startup.
///
/// `notify_to` doubles as the studio's public address: it is where order
/// notifications already go, and the address a customer should reply to.
pub fn init() {
    let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());

    let mail = match (
        var("SMTP_HOST"),
        var("SMTP_USER"),
        var("SMTP_PASSWORD"),
        var("NOTIFY_TO"),
        var("NOTIFY_FROM"),
    ) {
        (Some(host), Some(user), Some(password), Some(notify_to), Some(from)) => {
            let port = var("SMTP_PORT")
                .and_then(|p| p.trim().parse().ok())
                .unwrap_or(SmtpConfig::DEFAULT_PORT);
            let config = SmtpConfig {
                host,
                port,
                user,
                password,
                notify_to: notify_to.clone(),
                notify_from: from.clone(),
            };
            match SmtpNotifier::connect(&config) {
                Ok(notifier) => {
                    log::info!("SMTP ready, host={} port={}", config.host, config.port);
                    Mail {
                        notifier: Some(notifier),
                        fallback: FakeNotifier::new(),
                        studio: notify_to,
                        from,
                    }
                }
                Err(error) => {
                    log::error!("SMTP settings are unusable, mail will not be sent: {error}");
                    unconfigured(notify_to, from)
                }
            }
        }
        _ => {
            log::error!(
                "NO SMTP CONFIGURED — the back office will not email customers. \
                 Set SMTP_HOST, SMTP_USER, SMTP_PASSWORD, NOTIFY_TO and NOTIFY_FROM."
            );
            let studio = var("NOTIFY_TO").unwrap_or_else(|| "hej@hemsidor24.se".to_owned());
            let from = var("NOTIFY_FROM").unwrap_or_else(|| studio.clone());
            unconfigured(studio, from)
        }
    };

    let _ = MAIL.set(mail);
}

/// A `Mail` that can address a message but not send it.
fn unconfigured(studio: String, from: String) -> Mail {
    Mail {
        notifier: None,
        fallback: FakeNotifier::new(),
        studio,
        from,
    }
}

/// The configured mail, if [`init`] has run.
pub fn get() -> Option<&'static Mail> {
    MAIL.get()
}
