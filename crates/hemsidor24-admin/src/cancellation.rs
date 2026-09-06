//! Calling an order off, and paying the money back.
//!
//! The unhappy path, and the one the studio most needs a record of. Everything
//! up to here had a button; cancelling was a dropdown and a refund was a bank
//! transfer with nothing in the system to show it happened —
//! `deliveries.refunded_at` and `refund_reason` were columns nothing wrote.
//!
//! # Two actions, not one
//!
//! Cancelling and refunding are different facts. An order called off before a
//! proposal was ever sent owes nobody anything; one called off after the
//! customer paid needs money returned. Fusing them would either invent a refund
//! that never happened or hide one that did.
//!
//! # The guarantee ends at publication, and the domain already says so
//!
//! `OrderStatus::can_transition_to` allows Avbruten from Ny, Utkast skickat and
//! Godkänd — and not from Publicerad, which is terminal. That is exactly the
//! promise on the page: "Garantin gäller fram till publicering." Nothing here
//! restates the rule; it asks core, and core refuses.
//!
//! # Nothing here is signed
//!
//! Commercial settlement is application state. A refund is not a custody event,
//! and the handoff dialect has no vocabulary for one — deliberately. This
//! writes Postgres and sends mail, and that is the whole of it.

use hemsidor24_core::{OrderStatus, Package};
use rustio_admin::orm::{self, Db};
use rustio_admin::{BulkActionFailure, BulkActionResult, Result};

use crate::models::{Delivery, Order, Site};
use crate::workflow::{self, Step};

/// Cancelling: routed at `POST /admin/orders/bulk/<name>`.
pub const CANCEL_ACTION: &str = "cancel_order";

/// What the studio sees on the cancel button.
pub const CANCEL_LABEL: &str = "Avbryt beställning";

/// Recording a refund.
pub const REFUND_ACTION: &str = "record_refund";

/// What the studio sees on the refund button.
pub const REFUND_LABEL: &str = "Registrera återbetalning";

/// Cancelling marks no moment on the delivery: the delivery records what was
/// handed over, and a called-off order never reached that.
fn cancel_step() -> Step {
    Step {
        to: OrderStatus::Avbruten,
        stamp: None,
        message: |company, _package, to, from, studio| {
            hemsidor24_notify::order_cancelled(company, to, from, studio)
        },
    }
}

/// Call every selected order off.
pub async fn cancel_orders(db: &Db, ids: &[i64]) -> Result<BulkActionResult> {
    let step = cancel_step();
    let mut succeeded = 0;
    let mut failed = Vec::new();

    for &id in ids {
        match workflow::advance(db, id, &step).await {
            Ok(()) => succeeded += 1,
            Err(reason) => {
                log::warn!("{CANCEL_ACTION}: order {id} refused: {reason}");
                failed.push(BulkActionFailure::new(id, reason));
            }
        }
    }

    Ok(if failed.is_empty() {
        BulkActionResult::ok(succeeded)
    } else {
        BulkActionResult::partial(succeeded, failed)
    })
}

/// The delivery belonging to an order, if it has one.
async fn delivery_for(db: &Db, order_id: i64) -> std::result::Result<Delivery, String> {
    let site = orm::all::<Site>(db)
        .await
        .map_err(|e| format!("uppdragen kunde inte läsas: {e}"))?
        .into_iter()
        .find(|s| s.order_id == Some(order_id))
        .ok_or_else(|| "uppdraget saknas — det finns inget att återbetala mot".to_owned())?;

    orm::all::<Delivery>(db)
        .await
        .map_err(|e| format!("överlämningarna kunde inte läsas: {e}"))?
        .into_iter()
        .find(|d| d.site_id == Some(site.id))
        .ok_or_else(|| "överlämningen saknas".to_owned())
}

/// Record that money went back for one order.
///
/// Refuses unless the order has been cancelled. A refund on a live order would
/// mean the studio kept the site and returned the money, which is not a thing
/// this business does; if it ever becomes one it should be a decision somebody
/// makes explicitly, not a side effect of a button.
async fn refund_one(db: &Db, order_id: i64) -> std::result::Result<(), String> {
    let order = match orm::find::<Order>(db, order_id).await {
        Ok(Some(o)) => o,
        Ok(None) => return Err("beställningen finns inte".to_owned()),
        Err(e) => return Err(format!("kunde inte läsa beställningen: {e}")),
    };

    let status = OrderStatus::from_slug(&order.status)
        .ok_or_else(|| format!("okänd status {:?}", order.status))?;
    if status != OrderStatus::Avbruten {
        return Err(format!(
            "avbryt beställningen först — status är {}",
            status.label_sv()
        ));
    }

    let package = order
        .package
        .parse::<Package>()
        .map_err(|_| format!("okänt paket {:?}", order.package))?;

    let delivery = delivery_for(db, order_id).await?;
    let reason = delivery
        .refund_reason
        .clone()
        .filter(|r| !r.trim().is_empty())
        .ok_or_else(|| "fyll i refund_reason på överlämningen först".to_owned())?;
    if delivery.refunded_at.is_some() {
        return Err("återbetalningen är redan registrerad".to_owned());
    }

    // Write first.
    rustio_admin::sqlx::query(
        "UPDATE deliveries SET refunded_at = now() WHERE id = $1 AND refunded_at IS NULL",
    )
    .bind(delivery.id)
    .execute(db.pool())
    .await
    .map_err(|e| format!("återbetalningen kunde inte sparas: {e}"))?;

    // Notify second.
    match crate::mail::get() {
        Some(mail) => {
            let message = hemsidor24_notify::refund_issued(
                &order.company,
                package,
                &reason,
                &order.email,
                &mail.from,
                &mail.studio,
            );
            mail.send(&message).await;
        }
        None => log::error!("order {order_id}: mail was never initialised, customer not told"),
    }

    Ok(())
}

/// Run the refund over every selected order.
pub async fn record_refunds(db: &Db, ids: &[i64]) -> Result<BulkActionResult> {
    let mut succeeded = 0;
    let mut failed = Vec::new();

    for &id in ids {
        match refund_one(db, id).await {
            Ok(()) => succeeded += 1,
            Err(reason) => {
                log::warn!("{REFUND_ACTION}: order {id} refused: {reason}");
                failed.push(BulkActionFailure::new(id, reason));
            }
        }
    }

    Ok(if failed.is_empty() {
        BulkActionResult::ok(succeeded)
    } else {
        BulkActionResult::partial(succeeded, failed)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustio_admin::orm;
    use rustio_admin::{DateTime, ModelAdmin, Utc};

    async fn db() -> Option<Db> {
        crate::workflow::testdb::connect().await
    }

    async fn seed(db: &Db, company: &str, status: &str) -> i64 {
        let row: (i64,) = rustio_admin::sqlx::query_as(
            "INSERT INTO orders (company, city, phone, email, services, style, package, status)
             VALUES ($1, 'Malmö', '070-1', 'kund@example.se', 'Badrum', 'djarv', 'pro', $2)
             RETURNING id",
        )
        .bind(company)
        .bind(status)
        .fetch_one(db.pool())
        .await
        .expect("seed");
        row.0
    }

    async fn status_of(db: &Db, id: i64) -> String {
        orm::find::<Order>(db, id)
            .await
            .expect("read")
            .expect("order")
            .status
    }

    async fn refunded_at(db: &Db, order_id: i64) -> Option<DateTime<Utc>> {
        rustio_admin::sqlx::query_scalar(
            "SELECT d.refunded_at FROM deliveries d JOIN sites s ON d.site_id = s.id
              WHERE s.order_id = $1",
        )
        .bind(order_id)
        .fetch_one(db.pool())
        .await
        .expect("read refunded_at")
    }

    async fn set_reason(db: &Db, order_id: i64, reason: &str) {
        rustio_admin::sqlx::query(
            "UPDATE deliveries d SET refund_reason = $2
               FROM sites s WHERE d.site_id = s.id AND s.order_id = $1",
        )
        .bind(order_id)
        .bind(reason)
        .execute(db.pool())
        .await
        .expect("set reason");
    }

    /// Take an order all the way to a cancelled one that has a delivery.
    async fn cancelled_with_delivery(db: &Db, company: &str) -> i64 {
        let id = seed(db, company, "ny").await;
        crate::accept::accept_orders(db, &[id])
            .await
            .expect("customer and delivery");
        cancel_orders(db, &[id]).await.expect("cancel");
        id
    }

    #[tokio::test]
    async fn an_order_can_be_called_off_at_any_point_before_it_goes_live() {
        let Some(db) = db().await else { return };
        let _serial = crate::workflow::DB_LOCK.lock().await;

        // The promise on the page is that the guarantee runs until publication.
        // These expectations are that sentence, in table form.
        for (status, allowed) in [
            ("ny", true),
            ("utkast_skickat", true),
            ("godkand", true),
            ("publicerad", false),
            ("avbruten", false),
        ] {
            let id = seed(&db, "Cancel AB", status).await;
            let r = cancel_orders(&db, &[id]).await.expect("runs");
            assert_eq!(
                r.succeeded,
                usize::from(allowed),
                "status {status}: {:?}",
                r.failed
            );
            assert_eq!(
                status_of(&db, id).await,
                if allowed { "avbruten" } else { status },
                "status {status}"
            );
        }
    }

    #[tokio::test]
    async fn a_published_site_cannot_be_cancelled_out_from_under_the_customer() {
        let Some(db) = db().await else { return };
        let _serial = crate::workflow::DB_LOCK.lock().await;
        let id = seed(&db, "Live AB", "publicerad").await;

        let r = cancel_orders(&db, &[id]).await.expect("runs");
        assert_eq!(r.succeeded, 0);
        assert_eq!(r.failed.len(), 1);
        assert_eq!(status_of(&db, id).await, "publicerad");
    }

    #[tokio::test]
    async fn cancelling_an_order_that_never_became_a_job_still_works() {
        let Some(db) = db().await else { return };
        let _serial = crate::workflow::DB_LOCK.lock().await;
        // No customer, no site, no delivery — nothing to stamp, and that is
        // the ordinary case for an order called off the day it arrives.
        let id = seed(&db, "Early AB", "ny").await;
        let r = cancel_orders(&db, &[id]).await.expect("runs");
        assert_eq!(r.succeeded, 1, "{:?}", r.failed);
        assert_eq!(status_of(&db, id).await, "avbruten");
    }

    #[tokio::test]
    async fn money_is_only_returned_on_an_order_that_was_called_off() {
        let Some(db) = db().await else { return };
        let _serial = crate::workflow::DB_LOCK.lock().await;
        let id = seed(&db, "Live Refund AB", "ny").await;
        crate::accept::accept_orders(&db, &[id]).await.expect("job");
        set_reason(&db, id, "Kunden ångrade sig").await;

        let r = record_refunds(&db, &[id]).await.expect("runs");
        assert_eq!(r.succeeded, 0, "a live order is not refunded");
        assert!(refunded_at(&db, id).await.is_none());
    }

    #[tokio::test]
    async fn a_refund_needs_a_reason_before_it_can_be_recorded() {
        let Some(db) = db().await else { return };
        let _serial = crate::workflow::DB_LOCK.lock().await;
        let id = cancelled_with_delivery(&db, "NoReason AB").await;

        let r = record_refunds(&db, &[id]).await.expect("runs");
        assert_eq!(r.succeeded, 0, "no reason, no refund");
        assert!(refunded_at(&db, id).await.is_none());

        // Whitespace is not a reason either.
        set_reason(&db, id, "   ").await;
        let r = record_refunds(&db, &[id]).await.expect("runs");
        assert_eq!(r.succeeded, 0, "blank reason is still no reason");
        assert!(refunded_at(&db, id).await.is_none());
    }

    #[tokio::test]
    async fn a_recorded_refund_is_stamped_once_and_never_paid_twice() {
        let Some(db) = db().await else { return };
        let _serial = crate::workflow::DB_LOCK.lock().await;
        let id = cancelled_with_delivery(&db, "Refund AB").await;
        set_reason(&db, id, "Kunden ångrade sig").await;

        let r = record_refunds(&db, &[id]).await.expect("runs");
        assert_eq!(r.succeeded, 1, "{:?}", r.failed);
        let first = refunded_at(&db, id).await;
        assert!(first.is_some(), "the refund is stamped");

        // Running the action again is the realistic accident: two operators,
        // one list. It must refuse rather than move the date or send a second
        // mail telling the customer money is on its way again.
        let again = record_refunds(&db, &[id]).await.expect("runs");
        assert_eq!(again.succeeded, 0, "a refund is not paid twice");
        assert_eq!(first, refunded_at(&db, id).await, "the moment is unchanged");
    }

    #[tokio::test]
    async fn a_refund_without_a_job_behind_it_is_refused() {
        let Some(db) = db().await else { return };
        let _serial = crate::workflow::DB_LOCK.lock().await;
        // Cancelled before it ever became a job: there is no delivery to
        // stamp, so there is nothing to record a refund against.
        let id = seed(&db, "Bare AB", "avbruten").await;
        let r = record_refunds(&db, &[id]).await.expect("runs");
        assert_eq!(r.succeeded, 0);
        assert_eq!(r.failed.len(), 1);
    }

    #[test]
    fn both_actions_are_registered_confirm_first_and_are_marked_destructive() {
        let actions = <Order as ModelAdmin>::bulk_actions();
        for name in [CANCEL_ACTION, REFUND_ACTION] {
            let action = actions
                .iter()
                .find(|a| a.name == name)
                .unwrap_or_else(|| panic!("{name} is registered"));
            assert!(action.confirm, "{name} asks first");
            assert!(action.destructive, "{name} is marked destructive");
        }
    }

    #[test]
    fn every_declared_action_has_code_behind_it() {
        // The bug this guards against shipped once: a button was declared with
        // no handler, so it redirected, reported nothing, and changed nothing.
        // Both sides now come from one list, so the check is that the list has
        // no duplicate names quietly shadowing one another.
        let actions = <Order as ModelAdmin>::bulk_actions();
        let mut names: Vec<&str> = actions.iter().map(|a| a.name).collect();
        let declared = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(declared, names.len(), "action names are unique");
        assert_eq!(declared, 6, "the studio's whole workflow is registered");
    }
}
