//! Hemsidor24 back office.
//!
//! A second, separate server. Customers never reach it: it binds to its own
//! address and port, serves no public route, and shares nothing with
//! `hemsidor24-web` except the Postgres database underneath.
//!
//! The admin surface itself is `rustio-admin`, consumed exactly as published.
//! Nothing here patches, forks or works around it — where it did not fit, the
//! schema moved (see migrations 0002 and 0003), not the framework.

mod logging;
mod models;

use std::net::{IpAddr, SocketAddr};

use rustio_admin::admin::audit;
use rustio_admin::auth::{self, Role, create_user, find_user_by_email};
use rustio_admin::middleware;
use rustio_admin::templates::Templates;
use rustio_admin::{Admin, Db, Router, Server, register_admin_routes};
use thiserror::Error;

use crate::models::{Customer, Delivery, Domain, Order, Site};

/// Address the back office binds to when `ADMIN_BIND_ADDR` is unset.
///
/// Loopback, deliberately. This panel has no business being reachable from
/// anywhere but the machine it runs on, or whatever sits in front of it.
const DEFAULT_BIND_ADDR: &str = "127.0.0.1";

/// Port the back office listens on when `ADMIN_PORT` is unset.
const DEFAULT_PORT: u16 = 3001;

/// Shortest first-run password we will set up.
const MIN_PASSWORD_LEN: usize = 12;

/// Anything that can stop the back office from starting.
#[derive(Debug, Error)]
enum StartupError {
    /// A variable with no default was not set.
    #[error("{0} is required but not set")]
    Missing(&'static str),
    /// A variable was set to something unusable.
    #[error("{var} is not valid: {reason}")]
    Invalid {
        /// The variable that was wrong.
        var: &'static str,
        /// Why it could not be used.
        reason: &'static str,
    },
    /// The framework could not do something it needed to.
    #[error(transparent)]
    Admin(#[from] rustio_admin::Error),
}

/// Read a variable that must be present and non-empty.
fn required(var: &'static str) -> Result<String, StartupError> {
    match std::env::var(var) {
        Ok(v) if !v.trim().is_empty() => Ok(v),
        _ => Err(StartupError::Missing(var)),
    }
}

#[tokio::main]
async fn main() -> Result<(), StartupError> {
    logging::init();

    let database_url = required("DATABASE_URL")?;

    let host_raw =
        std::env::var("ADMIN_BIND_ADDR").unwrap_or_else(|_| DEFAULT_BIND_ADDR.to_owned());
    let host: IpAddr = host_raw.trim().parse().map_err(|_| StartupError::Invalid {
        var: "ADMIN_BIND_ADDR",
        reason: "expected an IP address such as 127.0.0.1",
    })?;

    let port = match std::env::var("ADMIN_PORT") {
        Err(_) => DEFAULT_PORT,
        Ok(raw) => raw
            .trim()
            .parse::<u16>()
            .map_err(|_| StartupError::Invalid {
                var: "ADMIN_PORT",
                reason: "expected a port number between 1 and 65535",
            })?,
    };

    let db = Db::connect(&database_url).await?;

    // The framework's own tables: users, sessions, permissions, audit. They
    // live alongside the domain tables in the same database but are owned by
    // rustio-admin, and are created here rather than in `/migrations` so the
    // framework stays the single authority on their shape.
    auth::init_tables(&db).await?;
    audit::ensure_table(&db).await?;

    ensure_first_administrator(&db).await?;

    let admin = Admin::new()
        .model::<Order>()
        .model::<Customer>()
        .model::<Domain>()
        .model::<Site>()
        .model::<Delivery>();

    let templates = Templates::new(None)?;

    // The framework ships these but registers none of them for you. Without
    // `csrf_protect` in particular the `rustio_csrf` cookie is never set, the
    // hidden `_csrf` field renders empty, and every form POST is rejected —
    // so the panel looks read-only. `correlation_id` must come before it, as
    // its own docs say, so audit rows and log lines share a request id.
    let router = Router::new()
        .middleware(middleware::correlation_id)
        .middleware(middleware::csrf_protect)
        .middleware(middleware::security_headers)
        .middleware(middleware::logger);

    let router = register_admin_routes(router, admin, db, templates);

    let addr = SocketAddr::new(host, port);
    println!("hemsidor24-admin listening on http://{addr}/admin/");
    Server::new(router, addr).run().await?;

    Ok(())
}

/// Create the first administrator, once, from the environment.
///
/// Only runs when `ADMIN_EMAIL` and `ADMIN_PASSWORD` are both set. After the
/// account exists, take them back out of the environment — leaving a password
/// in a process environment is worse than typing it once.
///
/// Everything else about accounts — additional users, roles, password changes,
/// recovery — is rustio-admin's own, and is done in the panel or with
/// `rustio-admin-cli`.
async fn ensure_first_administrator(db: &Db) -> Result<(), StartupError> {
    let (Ok(email), Ok(password)) = (required("ADMIN_EMAIL"), required("ADMIN_PASSWORD")) else {
        return Ok(());
    };

    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(StartupError::Invalid {
            var: "ADMIN_PASSWORD",
            reason: "expected at least 12 characters",
        });
    }

    // Idempotent: after the first run the account is there and this is a
    // no-op, so restarting the panel never resets a password.
    if find_user_by_email(db, &email).await?.is_some() {
        return Ok(());
    }

    let id = create_user(db, &email, &password, Role::Administrator).await?;
    println!("created administrator {email} (id {id})");
    Ok(())
}
