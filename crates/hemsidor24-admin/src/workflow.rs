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
