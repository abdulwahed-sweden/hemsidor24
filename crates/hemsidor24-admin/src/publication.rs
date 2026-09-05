//! Publishing a site, and signing what passed to the customer.
//!
//! The last step of an ordinary job, and the only one that changes anything
//! outside the studio's own records: a domain, a hosting account and a
//! repository stop being the studio's and become the customer's.
//!
//! Not built on [`crate::workflow::advance`], and that is worth saying because
//! the previous commit predicted it would be. Publishing needs the site row
//! before it can decide anything, stamps a column on `sites` rather than
//! `deliveries`, sets three flags conditionally, signs claims, and sends a
//! message that needs the live URL. Forcing that through a shared step would
//! have meant a step with five optional parameters, which is worse than two
//! honest callers and one that goes its own way.
//!
//! # What it requires, and why only that
//!
//! A live URL. That is what publication *is*, and it is the one thing the
//! customer is told. Everything else — domain, hosting reference, repository —
//! is signed if present and skipped if not, because a job where the customer
//! already owned their domain is a real job and should not be blocked by an
//! empty column.

use hemsidor24_core::OrderStatus;
use rustio_admin::orm::{self, Db};
use rustio_admin::{BulkActionFailure, BulkActionResult, Result};

use crate::models::{Order, Site};
use crate::signing::{self, Artefacts};

/// The bulk action's stable slug, routed at `POST /admin/orders/bulk/<name>`.
pub const ACTION: &str = "publish_site";

/// What the studio sees on the button.
pub const LABEL: &str = "Publicera och lämna över";

/// Find the site built for this order.
async fn site_for(db: &Db, order_id: i64) -> std::result::Result<Site, String> {
    orm::all::<Site>(db)
        .await
        .map_err(|e| format!("uppdragen kunde inte läsas: {e}"))?
        .into_iter()
        .find(|s| s.order_id == Some(order_id))
        .ok_or_else(|| "uppdraget saknas — kör \"Skapa kund och uppdrag\" först".to_owned())
}

/// The domain name attached to a site, if it has one.
async fn domain_name(db: &Db, site: &Site) -> Option<String> {
    let id = site.domain_id?;
    let name: Option<String> =
        rustio_admin::sqlx::query_scalar("SELECT name FROM domains WHERE id = $1")
            .bind(id)
            .fetch_optional(db.pool())
            .await
            .ok()
            .flatten();
    name
}

/// Publish one order's site.
async fn publish_one(db: &Db, order_id: i64) -> std::result::Result<(), String> {
    let order = match orm::find::<Order>(db, order_id).await {
        Ok(Some(o)) => o,
        Ok(None) => return Err("beställningen finns inte".to_owned()),
        Err(e) => return Err(format!("kunde inte läsa beställningen: {e}")),
    };

    let current = OrderStatus::from_slug(&order.status)
        .ok_or_else(|| format!("okänd status {:?}", order.status))?;
    if !current.can_transition_to(OrderStatus::Publicerad) {
        return Err(format!(
            "kan inte publicera när status är {}",
            current.label_sv()
        ));
    }

    let site = site_for(db, order_id).await?;
    let live_url = site
        .live_url
        .clone()
        .filter(|u| !u.trim().is_empty())
        .ok_or_else(|| "uppdraget saknar adress — fyll i live_url först".to_owned())?;

    let customer_id = order
        .customer_id
        .ok_or_else(|| "beställningen saknar kund".to_owned())?;

    let artefacts = Artefacts {
        domain: domain_name(db, &site).await,
        hosting: site.hosting_ref.clone().filter(|s| !s.trim().is_empty()),
        source: site.repo_url.clone().filter(|s| !s.trim().is_empty()),
    };
    if artefacts.is_empty() {
        log::warn!(
            "order {order_id}: publishing with nothing to hand over — no domain, \
             hosting reference or repository is recorded"
        );
    }

    // Write first: the site is live whether or not anything downstream works.
    rustio_admin::sqlx::query("UPDATE orders SET status = $1 WHERE id = $2")
        .bind(OrderStatus::Publicerad.slug())
        .bind(order.id)
        .execute(db.pool())
        .await
        .map_err(|e| format!("statusen kunde inte sparas: {e}"))?;

    if let Err(e) = rustio_admin::sqlx::query(
        "UPDATE sites SET published_at = now() WHERE id = $1 AND published_at IS NULL",
    )
    .bind(site.id)
    .execute(db.pool())
    .await
    {
        log::error!("order {order_id}: published_at could not be stamped: {e}");
    }

    // Sign, then record exactly what was signed. The flags follow the claims
    // rather than the intent, so `hosting_transferred` cannot end up true on
    // the strength of an empty column.
    let signed = signing::sign(customer_id, &artefacts).await;
    if let Err(e) = rustio_admin::sqlx::query(
        "UPDATE deliveries d
            SET domain_transferred  = d.domain_transferred  OR $2,
                hosting_transferred = d.hosting_transferred OR $3,
                source_transferred  = d.source_transferred  OR $4
           FROM sites s
          WHERE d.site_id = s.id AND s.order_id = $1",
    )
    .bind(order.id)
    .bind(signed.domain)
    .bind(signed.hosting)
    .bind(signed.source)
    .execute(db.pool())
    .await
    {
        log::error!("order {order_id}: transfer flags could not be recorded: {e}");
    }

    // Notify last.
    match crate::mail::get() {
        Some(mail) => {
            let message = hemsidor24_notify::site_published(
                &order.company,
                &live_url,
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

/// Run the action over every selected order.
pub async fn publish_sites(db: &Db, ids: &[i64]) -> Result<BulkActionResult> {
    let mut succeeded = 0;
    let mut failed = Vec::new();

    for &id in ids {
        match publish_one(db, id).await {
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
    use rustio_admin::{DateTime, ModelAdmin, Utc};

    async fn db() -> Option<Db> {
        let url = std::env::var("TEST_DATABASE_URL").ok()?;
        Db::connect(&url).await.ok()
    }

    /// An order walked as far as "Godkänd", with a site ready to publish.
    async fn ready(db: &Db, company: &str, live: Option<&str>) -> i64 {
        let row: (i64,) = rustio_admin::sqlx::query_as(
            "INSERT INTO orders (company, city, phone, email, services, style, package, status)
             VALUES ($1, 'Malmö', '070-1', 'kund@example.se', 'Badrum', 'djarv', 'pro', 'ny')
             RETURNING id",
        )
        .bind(company)
        .fetch_one(db.pool())
        .await
        .expect("seed");
        let id = row.0;

        crate::accept::accept_orders(db, &[id])
            .await
            .expect("accept");
        crate::proposal::send_proposals(db, &[id])
            .await
            .expect("propose");
        crate::acceptance::record_acceptances(db, &[id])
            .await
            .expect("accepted");

        if let Some(url) = live {
            rustio_admin::sqlx::query(
                "UPDATE sites SET live_url = $2, repo_url = $3, hosting_ref = $4
                  WHERE order_id = $1",
            )
            .bind(id)
            .bind(url)
            .bind("https://github.com/hemsidor24/example")
            .bind("loopia:558812")
            .execute(db.pool())
            .await
            .expect("fill the site");
        }
        id
    }

    async fn status_of(db: &Db, id: i64) -> String {
        orm::find::<Order>(db, id)
            .await
            .expect("read")
            .expect("order")
            .status
    }

    #[tokio::test]
    async fn publishing_needs_an_address() {
        let Some(db) = db().await else { return };
        let _serial = crate::workflow::DB_LOCK.lock().await;
        let id = ready(&db, "NoUrl AB", None).await;

        let r = publish_sites(&db, &[id]).await.expect("runs");
        assert_eq!(r.succeeded, 0);
        assert!(
            r.failed[0].reason.contains("live_url"),
            "{:?}",
            r.failed[0].reason
        );
        assert_eq!(status_of(&db, id).await, "godkand", "nothing moved");
    }

    #[tokio::test]
    async fn publishing_marks_the_site_live_and_stamps_it() {
        let Some(db) = db().await else { return };
        let _serial = crate::workflow::DB_LOCK.lock().await;
        let id = ready(&db, "Live AB", Some("https://live.example.se")).await;

        let r = publish_sites(&db, &[id]).await.expect("runs");
        assert_eq!(r.succeeded, 1, "{:?}", r.failed);
        assert_eq!(status_of(&db, id).await, "publicerad");

        let published: Option<DateTime<Utc>> =
            rustio_admin::sqlx::query_scalar("SELECT published_at FROM sites WHERE order_id = $1")
                .bind(id)
                .fetch_one(db.pool())
                .await
                .expect("read published_at");
        assert!(published.is_some(), "the site is stamped as published");
    }

    #[tokio::test]
    async fn only_an_accepted_order_can_be_published() {
        let Some(db) = db().await else { return };
        let _serial = crate::workflow::DB_LOCK.lock().await;
        for status in ["ny", "utkast_skickat", "publicerad", "avbruten"] {
            let id = ready(&db, "Rules AB", Some("https://x.example.se")).await;
            rustio_admin::sqlx::query("UPDATE orders SET status = $2 WHERE id = $1")
                .bind(id)
                .bind(status)
                .execute(db.pool())
                .await
                .expect("set status");

            let r = publish_sites(&db, &[id]).await.expect("runs");
            assert_eq!(r.succeeded, 0, "status {status} should not publish");
            assert_eq!(status_of(&db, id).await, status);
        }
    }

    #[tokio::test]
    async fn transfer_flags_follow_what_was_signed_not_what_was_hoped() {
        let Some(db) = db().await else { return };
        let _serial = crate::workflow::DB_LOCK.lock().await;
        let id = ready(&db, "Flags AB", Some("https://flags.example.se")).await;
        // No domain is linked to this site, so the domain can never be signed
        // for however the build is configured.
        publish_sites(&db, &[id]).await.expect("runs");

        let flags: (bool, bool, bool) = rustio_admin::sqlx::query_as(
            "SELECT d.domain_transferred, d.hosting_transferred, d.source_transferred
               FROM deliveries d JOIN sites s ON d.site_id = s.id WHERE s.order_id = $1",
        )
        .bind(id)
        .fetch_one(db.pool())
        .await
        .expect("read flags");
        assert!(
            !flags.0,
            "no domain is recorded, so none may be claimed transferred"
        );

        // Hosting and source are recorded, so they follow the build: signed
        // when the handover feature is on, and never claimed when it is off.
        let signing_on = cfg!(feature = "handover");
        assert_eq!(
            flags.1, signing_on,
            "hosting flag must match whether a claim was signed"
        );
        assert_eq!(
            flags.2, signing_on,
            "source flag must match whether a claim was signed"
        );
    }

    #[tokio::test]
    async fn publishing_twice_is_refused_and_does_not_restamp() {
        let Some(db) = db().await else { return };
        let _serial = crate::workflow::DB_LOCK.lock().await;
        let id = ready(&db, "Twice AB", Some("https://twice.example.se")).await;
        publish_sites(&db, &[id]).await.expect("first");
        let first: Option<DateTime<Utc>> =
            rustio_admin::sqlx::query_scalar("SELECT published_at FROM sites WHERE order_id = $1")
                .bind(id)
                .fetch_one(db.pool())
                .await
                .expect("read");

        let second = publish_sites(&db, &[id]).await.expect("second runs");
        assert_eq!(second.succeeded, 0, "Publicerad is terminal");
        let again: Option<DateTime<Utc>> =
            rustio_admin::sqlx::query_scalar("SELECT published_at FROM sites WHERE order_id = $1")
                .bind(id)
                .fetch_one(db.pool())
                .await
                .expect("read");
        assert_eq!(first, again, "the publication time is not rewritten");
    }

    #[test]
    fn all_four_actions_are_registered_and_confirm_first() {
        let actions = <Order as ModelAdmin>::bulk_actions();
        let names: Vec<&str> = actions.iter().map(|a| a.name).collect();
        for expected in [
            ACTION,
            crate::acceptance::ACTION,
            crate::proposal::ACTION,
            crate::accept::ACTION,
        ] {
            assert!(names.contains(&expected), "missing {expected}");
        }
        assert!(actions.iter().all(|a| a.confirm));
    }
}
