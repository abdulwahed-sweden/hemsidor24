//! Storing orders, and telling people about them.
//!
//! The ordering here is the whole point: **persist first, notify second**. An
//! order that reached Postgres is safe even if every mail server on the
//! internet is down, and the studio can still see it in the admin panel. An
//! order that was emailed but not stored is gone the moment the inbox is
//! tidied. So the database write is the one that is allowed to fail the
//! request; mail failures are logged loudly and swallowed.

use hemsidor24_core::Order;
use hemsidor24_notify::{Notifier, customer_confirmation, studio_notification};
use sqlx::PgPool;
use thiserror::Error;

/// The one failure the customer is allowed to see.
#[derive(Debug, Error)]
pub enum StoreError {
    /// The order could not be written.
    #[error("could not store the order")]
    Database(#[from] sqlx::Error),
}

/// Metadata the request carries that the domain does not care about.
#[derive(Debug, Clone, Default)]
pub struct RequestMeta {
    /// Client address as a string, hashed before it is stored.
    pub ip: Option<String>,
    /// `User-Agent` header, verbatim.
    pub user_agent: Option<String>,
}

/// Write the order and return its id.
///
/// The IP is hashed by Postgres with `sha256(ip || salt)` so the raw address
/// never lands in a column, a log line or a backup. It is kept only to spot
/// repeat spam, and the privacy policy promises 30 days.
pub async fn store(
    pool: &PgPool,
    order: &Order,
    meta: &RequestMeta,
    salt: &str,
) -> Result<i64, StoreError> {
    // Stored as the customer typed it: one service per line. Round-trips
    // exactly through `Services::parse`. See migration 0002 for why it is not
    // an array.
    let services = order.services().as_slice().join("\n");

    let row = sqlx::query!(
        r#"
        INSERT INTO orders (company, city, phone, email, services, style, package,
                            ip_hash, user_agent)
        VALUES ($1, $2, $3, $4, $5, $6, $7,
                CASE WHEN $8::text IS NULL THEN NULL
                     ELSE encode(sha256(($8::text || $9::text)::bytea), 'hex') END,
                $10)
        RETURNING id
        "#,
        order.company(),
        order.city(),
        order.phone(),
        order.email(),
        services,
        order.style().slug(),
        order.package().slug(),
        meta.ip.as_deref(),
        salt,
        meta.user_agent.as_deref(),
    )
    .fetch_one(pool)
    .await?;

    Ok(row.id)
}

/// Send the studio notification and the customer receipt.
///
/// Never returns an error. A mail failure is loud in the logs and invisible to
/// the customer, because by the time this runs their order is already stored.
pub async fn notify<N: Notifier>(
    pool: &PgPool,
    notifier: &N,
    order: &Order,
    order_id: i64,
    notify_to: &str,
    notify_from: &str,
) {
    let studio = studio_notification(order, order_id, notify_to, notify_from);
    match notifier.send(&studio).await {
        Ok(()) => {
            if let Err(error) = sqlx::query!(
                "UPDATE orders SET notified_at = now() WHERE id = $1",
                order_id
            )
            .execute(pool)
            .await
            {
                tracing::error!(order_id, %error, "order was emailed but notified_at was not set");
            }
        }
        Err(error) => tracing::error!(
            order_id,
            %error,
            "STUDIO NOTIFICATION FAILED — the order is stored, check the admin panel"
        ),
    }

    let receipt = customer_confirmation(order, notify_to, notify_from);
    if let Err(error) = notifier.send(&receipt).await {
        tracing::error!(order_id, %error, "customer confirmation failed to send");
    }
}

#[cfg(test)]
mod tests {
    use hemsidor24_core::OrderForm;
    use hemsidor24_notify::FakeNotifier;
    use sqlx::postgres::PgPoolOptions;

    use super::*;

    /// These need a real Postgres. Set `TEST_DATABASE_URL` to run them:
    ///
    /// ```sh
    /// TEST_DATABASE_URL=postgres://postgres@localhost/hemsidor24_test cargo test
    /// ```
    ///
    /// Without it they skip, so `cargo test` stays green on a laptop with no
    /// database. They are here rather than in `tests/` because they exercise
    /// the persist-then-notify ordering, which is the one rule in this crate
    /// worth protecting from a well-meaning refactor.
    async fn pool() -> Option<PgPool> {
        let url = std::env::var("TEST_DATABASE_URL").ok()?;
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .ok()?;
        sqlx::migrate!("../../migrations").run(&pool).await.ok()?;
        Some(pool)
    }

    fn order(company: &str) -> hemsidor24_core::Order {
        OrderForm {
            company: company.into(),
            city: "Malmö".into(),
            phone: "070-123 45 67".into(),
            email: "kontakt@example.se".into(),
            services: "Badrum\nKök".into(),
            style: "Djärv".into(),
            package: "Pro — 4 490 kr".into(),
        }
        .validate()
        .expect("fixture should be valid")
    }

    #[tokio::test]
    async fn an_order_survives_a_mail_outage() {
        let Some(pool) = pool().await else { return };
        let order = order("Mailfail AB");
        let meta = RequestMeta {
            ip: Some("198.51.100.7".into()),
            user_agent: Some("test".into()),
        };

        let id = store(&pool, &order, &meta, "test-salt-0123456789")
            .await
            .expect("stored");

        // Every send fails, and notify still returns normally.
        notify(
            &pool,
            &FakeNotifier::failing(),
            &order,
            id,
            "studio@x.se",
            "no-reply@x.se",
        )
        .await;

        // Selected as a boolean, not a timestamp: reading TIMESTAMPTZ into Rust
        // would mean adding a date/time crate, and nothing here needs one.
        let row = sqlx::query!(
            r#"SELECT company, (notified_at IS NOT NULL) AS "notified!" FROM orders WHERE id = $1"#,
            id
        )
        .fetch_one(&pool)
        .await
        .expect("the order must still be there");
        assert_eq!(row.company, "Mailfail AB");
        assert!(!row.notified, "a failed send must not look like a sent one");
    }

    #[tokio::test]
    async fn a_delivered_notification_is_recorded() {
        let Some(pool) = pool().await else { return };
        let order = order("Notified AB");
        let id = store(
            &pool,
            &order,
            &RequestMeta::default(),
            "test-salt-0123456789",
        )
        .await
        .expect("stored");

        let notifier = FakeNotifier::new();
        notify(&pool, &notifier, &order, id, "studio@x.se", "no-reply@x.se").await;

        let row = sqlx::query!(
            r#"SELECT (notified_at IS NOT NULL) AS "notified!" FROM orders WHERE id = $1"#,
            id
        )
        .fetch_one(&pool)
        .await
        .expect("row");
        assert!(row.notified);

        let sent = notifier.sent();
        assert_eq!(sent.len(), 2, "the studio and the customer both get one");
        assert_eq!(sent[0].to, "studio@x.se");
        assert_eq!(sent[1].to, "kontakt@example.se");
    }

    #[tokio::test]
    async fn the_raw_ip_is_never_stored() {
        let Some(pool) = pool().await else { return };
        let ip = "203.0.113.42";
        let meta = RequestMeta {
            ip: Some(ip.into()),
            user_agent: None,
        };
        let id = store(&pool, &order("Hashed AB"), &meta, "test-salt-0123456789")
            .await
            .expect("stored");

        let row = sqlx::query!("SELECT ip_hash FROM orders WHERE id = $1", id)
            .fetch_one(&pool)
            .await
            .expect("row");
        let hash = row
            .ip_hash
            .expect("an ip was supplied, so a hash was stored");
        assert_ne!(hash, ip);
        assert!(
            !hash.contains("203"),
            "the address must not survive in the hash"
        );
        assert_eq!(hash.len(), 64, "sha256, hex");
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[tokio::test]
    async fn a_missing_ip_stores_no_hash() {
        let Some(pool) = pool().await else { return };
        let id = store(
            &pool,
            &order("NoIp AB"),
            &RequestMeta::default(),
            "test-salt-0123456789",
        )
        .await
        .expect("stored");
        let row = sqlx::query!("SELECT ip_hash FROM orders WHERE id = $1", id)
            .fetch_one(&pool)
            .await
            .expect("row");
        assert!(row.ip_hash.is_none());
    }
}
