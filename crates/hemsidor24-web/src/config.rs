//! Server configuration, read from the environment.
//!
//! Nothing here has a default that could quietly cost an order. The database
//! URL is required, because an order that cannot be stored is an order lost.
//! SMTP is not required, because mail is best-effort by design — see
//! [`crate::orders`].

use std::net::{IpAddr, SocketAddr};

use hemsidor24_notify::SmtpConfig;
use thiserror::Error;

/// Address the server binds to when `BIND_ADDR` is unset.
const DEFAULT_BIND_ADDR: &str = "127.0.0.1";

/// Port the server listens on when `PORT` is unset.
const DEFAULT_PORT: u16 = 3000;

/// Shortest salt we will accept. An unsalted or barely salted hash of an IPv4
/// address is reversible by anyone willing to spend an afternoon on it.
const MIN_SALT_LEN: usize = 16;

/// Everything the server needs to start.
#[derive(Debug, Clone)]
pub struct Config {
    /// Where to listen.
    pub addr: SocketAddr,
    /// Postgres connection string.
    pub database_url: String,
    /// Salt mixed into the stored IP hash.
    pub ip_hash_salt: String,
    /// Mail settings, if they were all present.
    pub smtp: Option<SmtpConfig>,
}

/// An environment variable that was missing or unusable.
///
/// Unset-with-a-default is fine. Set to nonsense is not: silently falling back
/// to port 3000 when someone typed `PORT=800O` sends them hunting for a server
/// listening somewhere else entirely.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ConfigError {
    /// A variable with no default was not set.
    #[error("{0} is required but not set")]
    Missing(&'static str),
    /// A variable was set to something unusable.
    #[error("{var}={value:?} is not valid: {reason}")]
    Invalid {
        /// The variable that was wrong.
        var: &'static str,
        /// What it was set to.
        value: String,
        /// Why it could not be used.
        reason: &'static str,
    },
}

/// Read a variable that must be present and non-empty.
fn required(var: &'static str) -> Result<String, ConfigError> {
    match std::env::var(var) {
        Ok(v) if !v.trim().is_empty() => Ok(v),
        _ => Err(ConfigError::Missing(var)),
    }
}

/// Read a variable that may be absent.
fn optional(var: &str) -> Option<String> {
    std::env::var(var).ok().filter(|v| !v.trim().is_empty())
}

impl Config {
    /// Read the whole configuration from the environment.
    pub fn from_env() -> Result<Self, ConfigError> {
        let host_raw = std::env::var("BIND_ADDR").unwrap_or_else(|_| DEFAULT_BIND_ADDR.to_owned());
        let host: IpAddr = host_raw.trim().parse().map_err(|_| ConfigError::Invalid {
            var: "BIND_ADDR",
            value: host_raw.clone(),
            reason: "expected an IP address such as 127.0.0.1 or 0.0.0.0",
        })?;

        let port = match std::env::var("PORT") {
            Err(_) => DEFAULT_PORT,
            Ok(raw) => raw
                .trim()
                .parse::<u16>()
                .map_err(|_| ConfigError::Invalid {
                    var: "PORT",
                    value: raw.clone(),
                    reason: "expected a port number between 1 and 65535",
                })?,
        };

        let database_url = required("DATABASE_URL")?;

        let ip_hash_salt = required("IP_HASH_SALT")?;
        if ip_hash_salt.len() < MIN_SALT_LEN {
            return Err(ConfigError::Invalid {
                var: "IP_HASH_SALT",
                value: "<redacted>".to_owned(),
                reason: "expected at least 16 characters; generate one with `openssl rand -hex 32`",
            });
        }

        Ok(Config {
            addr: SocketAddr::new(host, port),
            database_url,
            ip_hash_salt,
            smtp: smtp_from_env()?,
        })
    }
}

/// Assemble the SMTP settings, or `None` if none of them are set.
///
/// Partially configured is treated as an error rather than as absent: half a
/// mail configuration is a typo, not a decision.
fn smtp_from_env() -> Result<Option<SmtpConfig>, ConfigError> {
    let vars = [
        "SMTP_HOST",
        "SMTP_USER",
        "SMTP_PASSWORD",
        "NOTIFY_TO",
        "NOTIFY_FROM",
    ];
    let present: Vec<&str> = vars
        .iter()
        .copied()
        .filter(|v| optional(v).is_some())
        .collect();

    if present.is_empty() {
        return Ok(None);
    }
    if present.len() < vars.len() {
        let missing = vars
            .iter()
            .find(|v| optional(v).is_none())
            .copied()
            .unwrap_or("SMTP_HOST");
        return Err(ConfigError::Missing(missing));
    }

    let port = match std::env::var("SMTP_PORT") {
        Err(_) => SmtpConfig::DEFAULT_PORT,
        Ok(raw) => raw
            .trim()
            .parse::<u16>()
            .map_err(|_| ConfigError::Invalid {
                var: "SMTP_PORT",
                value: raw.clone(),
                reason: "expected a port number; 587 for STARTTLS submission",
            })?,
    };

    Ok(Some(SmtpConfig {
        host: required("SMTP_HOST")?,
        port,
        user: required("SMTP_USER")?,
        password: required("SMTP_PASSWORD")?,
        notify_to: required("NOTIFY_TO")?,
        notify_from: required("NOTIFY_FROM")?,
    }))
}
