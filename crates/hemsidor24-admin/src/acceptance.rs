//! Recording that the customer said yes.
//!
//! The customer's approval arrived by email or over the phone and was recorded
//! by an operator changing a dropdown. Nothing captured when it came, and
//! `deliveries.accepted_at` was a column nothing wrote — so the moment the
//! guarantee stops applying, which is the moment that matters if anyone later
//! disagrees, was not written down anywhere.
//!
//! The mirror of [`crate::proposal`], and the same step underneath.

use hemsidor24_core::OrderStatus;
use rustio_admin::orm::Db;
use rustio_admin::{BulkActionFailure, BulkActionResult, Result};

use crate::workflow::{self, Step};

/// The bulk action's stable slug, routed at `POST /admin/orders/bulk/<name>`.
pub const ACTION: &str = "record_acceptance";

/// What the studio sees on the button.
pub const LABEL: &str = "Registrera godkännande";

/// Moving to "Godkänd" is the moment the customer accepted.
fn step() -> Step {
    Step {
        to: OrderStatus::Godkand,
        stamp: "accepted_at",
        message: hemsidor24_notify::proposal_accepted,
    }
}

/// Run the action over every selected order.
pub async fn record_acceptances(db: &Db, ids: &[i64]) -> Result<BulkActionResult> {
    let step = step();
    let mut succeeded = 0;
    let mut failed = Vec::new();

    for &id in ids {
        match workflow::advance(db, id, &step).await {
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
    use rustio_admin::orm;
    use rustio_admin::{DateTime, ModelAdmin, Utc};

    use crate::models::Order;

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

    async fn accepted_at(db: &Db, order_id: i64) -> Option<DateTime<Utc>> {
        rustio_admin::sqlx::query_scalar(
            "SELECT d.accepted_at FROM deliveries d JOIN sites s ON d.site_id = s.id
              WHERE s.order_id = $1",
        )
        .bind(order_id)
        .fetch_one(db.pool())
        .await
        .expect("read accepted_at")
    }

    #[tokio::test]
    async fn recording_an_acceptance_moves_a_proposed_order_to_godkand() {
        let Some(db) = db().await else { return };
        let _serial = crate::workflow::DB_LOCK.lock().await;
        let id = seed(&db, "Accepted AB", "utkast_skickat").await;

        let r = record_acceptances(&db, &[id]).await.expect("runs");
        assert_eq!(r.succeeded, 1, "{:?}", r.failed);
        assert_eq!(status_of(&db, id).await, "godkand");
    }

    #[tokio::test]
    async fn only_a_proposed_order_can_be_accepted() {
        let Some(db) = db().await else { return };
        let _serial = crate::workflow::DB_LOCK.lock().await;
        // Godkänd is reachable only from Utkast skickat.
        for (status, allowed) in [
            ("ny", false),
            ("utkast_skickat", true),
            ("godkand", false),
            ("publicerad", false),
            ("avbruten", false),
        ] {
            let id = seed(&db, "Rules AB", status).await;
            let r = record_acceptances(&db, &[id]).await.expect("runs");
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
    async fn the_moment_the_guarantee_closes_is_stamped_and_never_rewritten() {
        let Some(db) = db().await else { return };
        let _serial = crate::workflow::DB_LOCK.lock().await;
        let id = seed(&db, "Stamped AB", "ny").await;
        crate::accept::accept_orders(&db, &[id])
            .await
            .expect("accepted into a customer");
        crate::proposal::send_proposals(&db, &[id])
            .await
            .expect("proposed");

        record_acceptances(&db, &[id]).await.expect("runs");
        let first = accepted_at(&db, id).await;
        assert!(first.is_some(), "acceptance stamped the delivery");

        // Walk it back to a proposable state and run again: the original
        // acceptance time must survive.
        rustio_admin::sqlx::query("UPDATE orders SET status = 'utkast_skickat' WHERE id = $1")
            .bind(id)
            .execute(db.pool())
            .await
            .expect("reset");
        record_acceptances(&db, &[id]).await.expect("runs");
        assert_eq!(
            first,
            accepted_at(&db, id).await,
            "the first acceptance is not rewritten"
        );
    }

    #[tokio::test]
    async fn an_order_without_a_delivery_is_still_accepted() {
        let Some(db) = db().await else { return };
        let _serial = crate::workflow::DB_LOCK.lock().await;
        let id = seed(&db, "NoDelivery AB", "utkast_skickat").await;
        let r = record_acceptances(&db, &[id]).await.expect("runs");
        assert_eq!(r.succeeded, 1, "{:?}", r.failed);
        assert_eq!(status_of(&db, id).await, "godkand");
    }

    #[tokio::test]
    async fn the_whole_chain_runs_in_order() {
        let Some(db) = db().await else { return };
        let _serial = crate::workflow::DB_LOCK.lock().await;
        let id = seed(&db, "Chain AB", "ny").await;

        crate::accept::accept_orders(&db, &[id])
            .await
            .expect("accept");
        assert_eq!(
            status_of(&db, id).await,
            "ny",
            "accepting into a customer is not a status move"
        );

        crate::proposal::send_proposals(&db, &[id])
            .await
            .expect("propose");
        assert_eq!(status_of(&db, id).await, "utkast_skickat");

        record_acceptances(&db, &[id])
            .await
            .expect("accept proposal");
        assert_eq!(status_of(&db, id).await, "godkand");

        let stamps: (Option<DateTime<Utc>>, Option<DateTime<Utc>>) = rustio_admin::sqlx::query_as(
            "SELECT d.offered_at, d.accepted_at FROM deliveries d
               JOIN sites s ON d.site_id = s.id WHERE s.order_id = $1",
        )
        .bind(id)
        .fetch_one(db.pool())
        .await
        .expect("read stamps");
        let (offered, accepted) = stamps;
        assert!(
            offered.is_some() && accepted.is_some(),
            "both moments recorded"
        );
        assert!(
            offered <= accepted,
            "the offer cannot come after the acceptance"
        );
    }

    #[test]
    fn all_three_actions_are_registered_and_confirm_first() {
        let actions = <Order as ModelAdmin>::bulk_actions();
        let names: Vec<&str> = actions.iter().map(|a| a.name).collect();
        assert!(names.contains(&ACTION));
        assert!(names.contains(&crate::proposal::ACTION));
        assert!(names.contains(&crate::accept::ACTION));
        assert!(actions.iter().all(|a| a.confirm));
    }
}
