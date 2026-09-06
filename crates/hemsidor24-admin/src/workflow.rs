//! Moving an order one step along, and telling the customer.
//!
//! Two of the studio's actions are the same act with different nouns: check
//! that the domain rules allow the step, write the new status, stamp the moment
//! on the delivery, then mail the customer. Sending a proposal and recording an
//! acceptance differ only in which status they move to, which column they
//! stamp, and what the message says.
//!
//! This exists because that was about to be written a second time and would
//! have been written a third when publication arrives. It is not a framework:
//! it is one function with three parameters, and the two callers are a dozen
//! lines each.
//!
//! # The rule this enforces
//!
//! **Write first, notify second.** The status and the timestamp are in Postgres
//! before a message is attempted, so a mail outage can lose the sending and
//! never the fact. It is the same rule the public order form follows, for the
//! same reason: the database is the record, an inbox is not.

use hemsidor24_core::{OrderStatus, Package};
use hemsidor24_notify::Message;
use rustio_admin::orm::{self, Db};

use crate::models::Order;

/// One step of the studio's workflow.
pub struct Step {
    /// Where the order ends up. Whether it may get there is
    /// [`OrderStatus::can_transition_to`]'s decision, not this module's.
    pub to: OrderStatus,
    /// Column on `deliveries` to stamp with the current time, if the order has
    /// a delivery yet. A fixed name chosen at the call site — never input.
    ///
    /// `None` for a step that marks no moment on the delivery. Cancelling is
    /// one: the delivery records what was handed over, and an order that was
    /// called off never reached that.
    pub stamp: Option<&'static str>,
    /// What the customer is told. Both messages take the same shape, which is
    /// why a plain function pointer is enough.
    pub message: fn(&str, Package, &str, &str, &str) -> Message,
}

/// Advance one order, or explain why it cannot move.
pub async fn advance(db: &Db, order_id: i64, step: &Step) -> Result<(), String> {
    let order = match orm::find::<Order>(db, order_id).await {
        Ok(Some(o)) => o,
        Ok(None) => return Err("beställningen finns inte".to_owned()),
        Err(e) => return Err(format!("kunde inte läsa beställningen: {e}")),
    };

    let current = OrderStatus::from_slug(&order.status)
        .ok_or_else(|| format!("okänd status {:?}", order.status))?;
    if !current.can_transition_to(step.to) {
        return Err(format!(
            "kan inte gå till {} när status är {}",
            step.to.label_sv(),
            current.label_sv()
        ));
    }

    let package = order
        .package
        .parse::<Package>()
        .map_err(|_| format!("okänt paket {:?}", order.package))?;

    // Write first.
    rustio_admin::sqlx::query("UPDATE orders SET status = $1 WHERE id = $2")
        .bind(step.to.slug())
        .bind(order.id)
        .execute(db.pool())
        .await
        .map_err(|e| format!("statusen kunde inte sparas: {e}"))?;

    if let Some(column) = step.stamp {
        stamp_delivery(db, order.id, column).await;
    }

    // Notify second.
    match crate::mail::get() {
        Some(mail) => {
            let message = (step.message)(
                &order.company,
                package,
                &order.email,
                &mail.from,
                &mail.studio,
            );
            mail.send(&message).await;
        }
        None => log::error!(
            "order {}: mail was never initialised, customer not told",
            order.id
        ),
    }

    Ok(())
}

/// Stamp the moment on the order's delivery, if it has one and the column is
/// still empty.
///
/// Absence is not a failure. An order can be proposed before it is accepted, in
/// which case `accept_order` has not run and there is no delivery row yet; the
/// status change and the message are the substance. `IS NULL` so a repeat can
/// never rewrite the first time something happened.
///
/// The column name is a `&'static str` chosen in this crate and never derived
/// from input, which is what makes interpolating it safe.
async fn stamp_delivery(db: &Db, order_id: i64, column: &'static str) {
    let sql = format!(
        "UPDATE deliveries d SET {column} = now()
           FROM sites s
          WHERE d.site_id = s.id AND s.order_id = $1 AND d.{column} IS NULL"
    );
    if let Err(e) = rustio_admin::sqlx::query(rustio_admin::sqlx::AssertSqlSafe(sql))
        .bind(order_id)
        .execute(db.pool())
        .await
    {
        // The status is already saved; a missing timestamp is worth a loud log
        // and not worth failing the operator's action over.
        log::error!("order {order_id}: {column} could not be stamped: {e}");
    }
}

/// Serialises the database-backed tests.
///
/// They all run against one schema, and several of them count rows or read a
/// delivery they just stamped. Left parallel they interfere: a count taken
/// between another test's insert and its assertion is a failure that has
/// nothing to do with the code under test. One lock is cheaper than making
/// every test build its own schema, and a flaky suite is worth less than a slow
/// one.
#[cfg(test)]
pub static DB_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Everything the database-backed tests need to reach Postgres safely.
///
/// The tests insert, update and truncate freely. Pointed at the development
/// database they would destroy real work, and they would do it while reporting
/// success — which is why the mismatch is a panic and not a skip.
#[cfg(test)]
pub mod testdb {
    use rustio_admin::orm::Db;

    /// The database the tests may use, or `None` when none is configured.
    ///
    /// Panics when `TEST_DATABASE_URL` names the same database as
    /// `DATABASE_URL`. Skipping would be the gentler failure and the wrong
    /// one: a developer who set both to the same value would get a green run
    /// and a wrecked development dataset, and would have no reason to look.
    pub fn url() -> Option<String> {
        let test = std::env::var("TEST_DATABASE_URL")
            .ok()
            .filter(|v| !v.trim().is_empty())?;

        if let Ok(dev) = std::env::var("DATABASE_URL")
            && same_target(&test, &dev)
        {
            panic!(
                "TEST_DATABASE_URL and DATABASE_URL both name {}. \
                 These tests write and truncate, so running them would destroy \
                 development data. Point TEST_DATABASE_URL at a dedicated \
                 database, for example hemsidor24_test.",
                target(&test).unwrap_or_else(|| "the same database".to_owned())
            );
        }
        Some(test)
    }

    /// Connect to the test database, or `None` to skip.
    pub async fn connect() -> Option<Db> {
        Db::connect(&url()?).await.ok()
    }

    /// `host:port/database`, with the default port made explicit so that
    /// `localhost/db` and `localhost:5432/db` compare equal.
    fn target(url: &str) -> Option<String> {
        let rest = url.split_once("://")?.1;
        let (authority, path) = rest.split_once('/')?;
        let database = path.split(['?', '#']).next()?;
        let hostport = authority.rsplit('@').next()?;
        // A bracketed IPv6 literal keeps its colons; only a trailing :port counts.
        let (host, port) = match hostport.rsplit_once(':') {
            Some((h, p)) if !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()) => (h, p),
            _ => (hostport, "5432"),
        };
        Some(format!("{host}:{port}/{database}"))
    }

    fn same_target(a: &str, b: &str) -> bool {
        match (target(a), target(b)) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn the_default_port_is_made_explicit_before_comparing() {
            assert!(same_target(
                "postgres://postgres@localhost/hemsidor24",
                "postgres://postgres@localhost:5432/hemsidor24"
            ));
        }

        #[test]
        fn credentials_do_not_change_which_database_is_named() {
            assert!(same_target(
                "postgres://alice:secret@db.example:5432/hemsidor24",
                "postgres://bob@db.example:5432/hemsidor24"
            ));
        }

        #[test]
        fn a_dedicated_test_database_is_a_different_target() {
            assert!(!same_target(
                "postgres://postgres@localhost:5432/hemsidor24_test",
                "postgres://postgres@localhost:5432/hemsidor24"
            ));
        }

        #[test]
        fn query_parameters_are_not_part_of_the_name() {
            assert!(same_target(
                "postgres://postgres@localhost/hemsidor24?sslmode=require",
                "postgres://postgres@localhost:5432/hemsidor24"
            ));
        }

        #[test]
        fn a_different_host_or_port_is_a_different_target() {
            assert!(!same_target(
                "postgres://postgres@localhost:5433/hemsidor24",
                "postgres://postgres@localhost:5432/hemsidor24"
            ));
            assert!(!same_target(
                "postgres://postgres@other.host/hemsidor24",
                "postgres://postgres@localhost/hemsidor24"
            ));
        }
    }
}
