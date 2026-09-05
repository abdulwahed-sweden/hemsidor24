//! Turning an accepted order into the records the rest of the job needs.
//!
//! Everything to the right of "order arrives" in the studio's workflow —
//! customer, site, delivery — existed as tables the back office could show and
//! nothing could fill. Accepting an order meant retyping the company, city,
//! phone and email that were already sitting in the order row, inventing three
//! more rows, and then typing the row ids into each other by hand. Four
//! chances to mistype a customer's own contact details, on every job.
//!
//! This closes that. One action on the order list creates the three rows,
//! copies what the customer already told us, and links them.
//!
//! It is deliberately not automatic on a status change. The studio may want a
//! customer registered before the proposal is approved — a domain often has to
//! be bought first — and an irreversible side effect hanging off a dropdown is
//! the kind of thing that surprises people. This is an explicit act, and
//! rustio-admin records it in the audit trail like any other.

use rustio_admin::orm::{self, Db};
use rustio_admin::{BulkActionFailure, BulkActionResult, Result, Utc};

use crate::models::{Customer, Delivery, Order, Site};

/// The bulk action's stable slug, routed at `POST /admin/orders/bulk/<name>`.
pub const ACTION: &str = "accept_order";

/// What the studio sees on the button.
pub const LABEL: &str = "Skapa kund och uppdrag";

/// Create the customer, site and delivery for one order, and link them.
///
/// Refuses rather than guesses:
///
/// - an order already linked to a customer is left alone, so running the action
///   twice cannot produce a second customer for the same job;
/// - a cancelled order is refused, because a customer record built from one is
///   a mistake nobody would notice until it was invoiced.
///
/// Not a transaction. Postgres would give us one, but rustio-admin's ORM works
/// against the pool rather than a handle we can borrow, and reaching around it
/// to open one would mean this crate keeping its own idea of how the framework
/// talks to the database. The order it writes in is the order that degrades
/// safely: customer, then site, then delivery, then the link back. A failure
/// part-way leaves rows that are visible and editable in the panel rather than
/// a half-linked order that looks finished.
async fn accept_one(db: &Db, order_id: i64) -> std::result::Result<(), String> {
    let order = match orm::find::<Order>(db, order_id).await {
        Ok(Some(o)) => o,
        Ok(None) => return Err("beställningen finns inte".to_owned()),
        Err(e) => return Err(format!("kunde inte läsa beställningen: {e}")),
    };

    if order.customer_id.is_some() {
        return Err("har redan en kund".to_owned());
    }
    if order.status == "avbruten" {
        return Err("beställningen är avbruten".to_owned());
    }

    let now = Utc::now();

    let customer_id = orm::create(
        db,
        &Customer {
            id: 0,
            company: order.company.clone(),
            city: order.city.clone(),
            phone: order.phone.clone(),
            email: order.email.clone(),
            org_nr: None,
            notes: None,
            created_at: now,
        },
    )
    .await
    .map_err(|e| format!("kunden kunde inte skapas: {e}"))?;

    let site_id = orm::create(
        db,
        &Site {
            id: 0,
            customer_id: Some(customer_id),
            order_id: Some(order.id),
            domain_id: None,
            repo_url: None,
            live_url: None,
            hosting_ref: None,
            package: order.package.clone(),
            revisions_used: 0,
            published_at: None,
            notes: None,
            created_at: now,
        },
    )
    .await
    .map_err(|e| format!("uppdraget kunde inte skapas: {e}"))?;

    orm::create(
        db,
        &Delivery {
            id: 0,
            site_id: Some(site_id),
            customer_id: Some(customer_id),
            offered_at: None,
            accepted_at: None,
            domain_transferred: false,
            hosting_transferred: false,
            source_transferred: false,
            refunded_at: None,
            refund_reason: None,
            notes: None,
            created_at: now,
        },
    )
    .await
    .map_err(|e| format!("överlämningen kunde inte skapas: {e}"))?;

    // Only the link, not the whole row: the order's own columns are the
    // customer's words and have no business being rewritten here.
    rustio_admin::sqlx::query("UPDATE orders SET customer_id = $1 WHERE id = $2")
        .bind(customer_id)
        .bind(order.id)
        .execute(db.pool())
        .await
        .map_err(|e| format!("beställningen kunde inte kopplas till kunden: {e}"))?;

    Ok(())
}

/// Run the action over every selected order.
pub async fn accept_orders(db: &Db, ids: &[i64]) -> Result<BulkActionResult> {
    let mut succeeded = 0;
    let mut failed = Vec::new();

    for &id in ids {
        match accept_one(db, id).await {
            Ok(()) => succeeded += 1,
            Err(reason) => {
                log::warn!("{ACTION}: order {id} refused: {reason}");
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
    use rustio_admin::ModelAdmin;
    use rustio_admin::orm::Model;

    /// Needs a database. Set `TEST_DATABASE_URL` to run these; without it they
    /// return early, so `cargo test` stays green on a laptop with no Postgres.
    async fn db() -> Option<Db> {
        let url = std::env::var("TEST_DATABASE_URL").ok()?;
        Db::connect(&url).await.ok()
    }

    async fn seed_order(db: &Db, company: &str, status: &str) -> i64 {
        let row: (i64,) = rustio_admin::sqlx::query_as(
            "INSERT INTO orders (company, city, phone, email, services, style, package, status)
             VALUES ($1, 'Malmö', '070-123 45 67', 'kontakt@example.se',
                     'Badrum', 'djarv', 'pro', $2)
             RETURNING id",
        )
        .bind(company)
        .bind(status)
        .fetch_one(db.pool())
        .await
        .expect("seed order");
        row.0
    }

    #[tokio::test]
    async fn accepting_an_order_creates_and_links_the_three_rows() {
        let Some(db) = db().await else { return };
        let _serial = crate::workflow::DB_LOCK.lock().await;
        let order_id = seed_order(&db, "Accept AB", "godkand").await;

        let result = accept_orders(&db, &[order_id]).await.expect("action runs");
        assert_eq!(result.succeeded, 1, "{:?}", result.failed);
        assert!(result.failed.is_empty());

        let order = orm::find::<Order>(&db, order_id)
            .await
            .expect("read")
            .expect("order");
        let customer_id = order
            .customer_id
            .expect("the order is now linked to a customer");

        let customer = orm::find::<Customer>(&db, customer_id)
            .await
            .expect("read")
            .expect("customer");
        assert_eq!(
            customer.company, "Accept AB",
            "contact details are copied, not retyped"
        );
        assert_eq!(customer.city, "Malmö");
        assert_eq!(customer.phone, "070-123 45 67");
        assert_eq!(customer.email, "kontakt@example.se");

        let sites = orm::all::<Site>(&db).await.expect("sites");
        let site = sites
            .iter()
            .find(|s| s.order_id == Some(order_id))
            .expect("site created");
        assert_eq!(site.customer_id, Some(customer_id));
        assert_eq!(
            site.package, "pro",
            "the package the customer chose carries over"
        );
        assert_eq!(site.revisions_used, 0);

        let deliveries = orm::all::<Delivery>(&db).await.expect("deliveries");
        let delivery = deliveries
            .iter()
            .find(|d| d.site_id == Some(site.id))
            .expect("delivery created");
        assert_eq!(delivery.customer_id, Some(customer_id));
        assert!(delivery.offered_at.is_none() && delivery.accepted_at.is_none());
        assert!(
            !delivery.domain_transferred
                && !delivery.hosting_transferred
                && !delivery.source_transferred
        );
    }

    #[tokio::test]
    async fn running_it_twice_does_not_create_a_second_customer() {
        let Some(db) = db().await else { return };
        let _serial = crate::workflow::DB_LOCK.lock().await;
        let order_id = seed_order(&db, "Twice AB", "godkand").await;

        assert_eq!(
            accept_orders(&db, &[order_id])
                .await
                .expect("first")
                .succeeded,
            1
        );
        let before = orm::all::<Customer>(&db).await.expect("customers").len();

        let second = accept_orders(&db, &[order_id]).await.expect("second runs");
        assert_eq!(second.succeeded, 0);
        assert_eq!(second.failed.len(), 1);
        assert!(
            second.failed[0].reason.contains("redan"),
            "{:?}",
            second.failed[0].reason
        );

        assert_eq!(
            orm::all::<Customer>(&db).await.expect("customers").len(),
            before,
            "no second customer was created"
        );
    }

    #[tokio::test]
    async fn a_cancelled_order_is_refused() {
        let Some(db) = db().await else { return };
        let _serial = crate::workflow::DB_LOCK.lock().await;
        let order_id = seed_order(&db, "Cancelled AB", "avbruten").await;

        let result = accept_orders(&db, &[order_id]).await.expect("action runs");
        assert_eq!(result.succeeded, 0);
        assert_eq!(result.failed.len(), 1);
        assert!(result.failed[0].reason.contains("avbruten"));

        let order = orm::find::<Order>(&db, order_id)
            .await
            .expect("read")
            .expect("order");
        assert!(order.customer_id.is_none(), "nothing was linked");
    }

    #[tokio::test]
    async fn one_bad_order_does_not_stop_the_others() {
        let Some(db) = db().await else { return };
        let _serial = crate::workflow::DB_LOCK.lock().await;
        let good = seed_order(&db, "Good AB", "ny").await;
        let bad = seed_order(&db, "Bad AB", "avbruten").await;

        let result = accept_orders(&db, &[good, bad]).await.expect("action runs");
        assert_eq!(result.succeeded, 1);
        assert_eq!(result.failed.len(), 1);
        assert_eq!(result.failed[0].id, bad);
        assert_eq!(result.total(), 2);

        assert!(
            orm::find::<Order>(&db, good)
                .await
                .expect("r")
                .expect("o")
                .customer_id
                .is_some()
        );
        assert!(
            orm::find::<Order>(&db, bad)
                .await
                .expect("r")
                .expect("o")
                .customer_id
                .is_none()
        );
    }

    #[test]
    fn the_action_is_registered_on_the_order_model() {
        let action = <Order as ModelAdmin>::bulk_actions()
            .iter()
            .find(|a| a.name == ACTION)
            .expect("the accept action is registered");
        assert_eq!(action.label, LABEL);
        assert!(
            action.confirm,
            "an irreversible create should confirm first"
        );
        assert_eq!(Order::TABLE, "orders");
    }
}
