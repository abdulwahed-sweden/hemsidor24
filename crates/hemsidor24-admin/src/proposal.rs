//! Telling the customer their proposal is ready.
//!
//! Between placing an order and the site appearing, the customer heard nothing.
//! The product's central promise — a proposal within a working day, seen before
//! any money changes hands — depended on somebody remembering to write an email
//! by hand, and nothing in the system recorded whether they had.
//!
//! This is the same shape as [`crate::accept`]: an explicit action on the order
//! list, because rustio-admin has no post-save hook and because a status
//! dropdown that quietly emails a customer is not something an operator can
//! take back.
//!
//! Order of work is the rule the public form already follows: **write first,
//! notify second**. The status change and the timestamp are in Postgres before
//! a message is attempted, so a mail outage cannot lose the fact that the
//! proposal was sent — it can only lose the sending, which is logged loudly.

use hemsidor24_core::{OrderStatus, Package};
use rustio_admin::orm::{self, Db};
use rustio_admin::{BulkActionFailure, BulkActionResult, Result};

use crate::models::Order;

/// The bulk action's stable slug, routed at `POST /admin/orders/bulk/<name>`.
pub const ACTION: &str = "send_proposal";

/// What the studio sees on the button.
pub const LABEL: &str = "Skicka utkast till kund";

/// Move one order to "Utkast skickat", stamp the delivery, and mail the
/// customer.
///
/// The transition is checked against [`OrderStatus`] rather than against a
/// string here, so the back office cannot walk an order somewhere the domain
/// rules forbid — a published order cannot be sent a proposal, and neither can
/// a cancelled one.
async fn send_one(db: &Db, order_id: i64) -> std::result::Result<(), String> {
    let order = match orm::find::<Order>(db, order_id).await {
        Ok(Some(o)) => o,
        Ok(None) => return Err("beställningen finns inte".to_owned()),
        Err(e) => return Err(format!("kunde inte läsa beställningen: {e}")),
    };

    let current = OrderStatus::from_slug(&order.status)
        .ok_or_else(|| format!("okänd status {:?}", order.status))?;
    if !current.can_transition_to(OrderStatus::UtkastSkickat) {
        return Err(format!(
            "kan inte skicka utkast när status är {}",
            current.label_sv()
        ));
    }

    let package = order
        .package
        .parse::<Package>()
        .map_err(|_| format!("okänt paket {:?}", order.package))?;

    // Write first.
    rustio_admin::sqlx::query("UPDATE orders SET status = $1 WHERE id = $2")
        .bind(OrderStatus::UtkastSkickat.slug())
        .bind(order.id)
        .execute(db.pool())
        .await
        .map_err(|e| format!("statusen kunde inte sparas: {e}"))?;

    // Stamp the delivery if one exists yet. It will not for an order that has
    // not been accepted — the delivery row is created by `accept_order` — and
    // that is not a failure: the status change and the mail are the substance,
    // and an order can legitimately be proposed before it is accepted.
    // `offered_at IS NULL` so re-sending never rewrites the first offer.
    if let Err(e) = rustio_admin::sqlx::query(
        "UPDATE deliveries d SET offered_at = now()
           FROM sites s
          WHERE d.site_id = s.id AND s.order_id = $1 AND d.offered_at IS NULL",
    )
    .bind(order.id)
    .execute(db.pool())
    .await
    {
        // The status is already saved; a missing timestamp is worth a loud log
        // and not worth failing the operator's action over.
        log::error!("order {}: offered_at could not be stamped: {e}", order.id);
    }

    // Notify second.
    if let Some(mail) = crate::mail::get() {
        let message = hemsidor24_notify::proposal_ready(
            &order.company,
            package,
            &order.email,
            &mail.from,
            &mail.studio,
        );
        mail.send(&message).await;
    } else {
        log::error!(
            "order {}: mail was never initialised, customer not told",
            order.id
        );
    }

    Ok(())
}

/// Run the action over every selected order.
pub async fn send_proposals(db: &Db, ids: &[i64]) -> Result<BulkActionResult> {
    let mut succeeded = 0;
    let mut failed = Vec::new();

    for &id in ids {
        match send_one(db, id).await {
            Ok(()) => succeeded += 1,
            Err(reason) => failed.push(BulkActionFailure::new(id, reason)),
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
    use rustio_admin::ModelAdmin;

    async fn db() -> Option<Db> {
        let url = std::env::var("TEST_DATABASE_URL").ok()?;
        Db::connect(&url).await.ok()
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

    #[tokio::test]
    async fn sending_a_proposal_moves_a_new_order_to_utkast_skickat() {
        let Some(db) = db().await else { return };
        let id = seed(&db, "Proposal AB", "ny").await;

        let result = send_proposals(&db, &[id]).await.expect("runs");
        assert_eq!(result.succeeded, 1, "{:?}", result.failed);
        assert_eq!(status_of(&db, id).await, "utkast_skickat");
    }

    #[tokio::test]
    async fn the_domain_rules_decide_which_orders_can_be_proposed() {
        let Some(db) = db().await else { return };
        // Ny is the only status core allows to reach Utkast skickat.
        for (status, allowed) in [
            ("ny", true),
            ("utkast_skickat", false),
            ("godkand", false),
            ("publicerad", false),
            ("avbruten", false),
        ] {
            let id = seed(&db, "Rules AB", status).await;
            let r = send_proposals(&db, &[id]).await.expect("runs");
            assert_eq!(
                r.succeeded,
                usize::from(allowed),
                "status {status}: {:?}",
                r.failed
            );
            if !allowed {
                assert_eq!(
                    status_of(&db, id).await,
                    status,
                    "status {status} was not changed"
                );
            }
        }
    }

    #[tokio::test]
    async fn the_delivery_is_stamped_when_one_exists_and_is_not_rewritten() {
        let Some(db) = db().await else { return };
        let order_id = seed(&db, "Stamped AB", "ny").await;
        // Accepting first creates the site and delivery this order will stamp.
        crate::accept::accept_orders(&db, &[order_id])
            .await
            .expect("accepted");

        send_proposals(&db, &[order_id]).await.expect("runs");
        let first: Option<rustio_admin::DateTime<rustio_admin::Utc>> =
            rustio_admin::sqlx::query_scalar(
                "SELECT d.offered_at FROM deliveries d JOIN sites s ON d.site_id = s.id
              WHERE s.order_id = $1",
            )
            .bind(order_id)
            .fetch_one(db.pool())
            .await
            .expect("read offered_at");
        assert!(first.is_some(), "the proposal stamped the delivery");

        // Re-sending is refused by the status rules, so the stamp cannot move;
        // prove the guard holds by running the update path again directly.
        rustio_admin::sqlx::query("UPDATE orders SET status = 'ny' WHERE id = $1")
            .bind(order_id)
            .execute(db.pool())
            .await
            .expect("reset");
        send_proposals(&db, &[order_id]).await.expect("runs");
        let second: Option<rustio_admin::DateTime<rustio_admin::Utc>> =
            rustio_admin::sqlx::query_scalar(
                "SELECT d.offered_at FROM deliveries d JOIN sites s ON d.site_id = s.id
              WHERE s.order_id = $1",
            )
            .bind(order_id)
            .fetch_one(db.pool())
            .await
            .expect("read offered_at");
        assert_eq!(
            first, second,
            "the first offer is not rewritten by a later send"
        );
    }

    #[tokio::test]
    async fn an_order_without_a_delivery_still_gets_its_proposal_sent() {
        let Some(db) = db().await else { return };
        // No accept_order first, so there is no site and no delivery.
        let id = seed(&db, "NoDelivery AB", "ny").await;
        let r = send_proposals(&db, &[id]).await.expect("runs");
        assert_eq!(r.succeeded, 1, "{:?}", r.failed);
        assert_eq!(status_of(&db, id).await, "utkast_skickat");
    }

    #[tokio::test]
    async fn one_ineligible_order_does_not_stop_the_others() {
        let Some(db) = db().await else { return };
        let good = seed(&db, "Good AB", "ny").await;
        let bad = seed(&db, "Bad AB", "publicerad").await;

        let r = send_proposals(&db, &[good, bad]).await.expect("runs");
        assert_eq!(r.succeeded, 1);
        assert_eq!(r.failed.len(), 1);
        assert_eq!(r.failed[0].id, bad);
        assert_eq!(status_of(&db, good).await, "utkast_skickat");
        assert_eq!(status_of(&db, bad).await, "publicerad");
    }

    #[test]
    fn both_actions_are_registered_on_the_order_model() {
        let names: Vec<&str> = <Order as ModelAdmin>::bulk_actions()
            .iter()
            .map(|a| a.name)
            .collect();
        assert!(names.contains(&ACTION));
        assert!(names.contains(&crate::accept::ACTION));
        assert!(
            <Order as ModelAdmin>::bulk_actions()
                .iter()
                .all(|a| a.confirm)
        );
    }
}
