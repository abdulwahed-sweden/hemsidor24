//! The five entities the back office works with.
//!
//! Each struct mirrors one table, field for field, in column order — that is
//! not a style choice: `#[derive(RustioAdmin)]` builds `Model::COLUMNS` and
//! `INSERT_COLUMNS` from the field walk, so the struct *is* the schema as far
//! as the admin is concerned. Add a column in a migration, add a field here.
//!
//! Nothing in this file reaches into `rustio-admin`. It is consumed exactly as
//! published: derive, `impl ModelAdmin`, register.

use rustio_admin::{
    BulkAction, BulkActionContext, BulkActionResult, DateTime, Db, ModelAdmin, Result, RustioAdmin,
    Utc,
};

/// An order as it arrived from the public form.
///
/// Read-mostly. The studio changes `status` and links a `customer_id`; the
/// rest is what the customer typed and must not be edited out from under them.
#[derive(RustioAdmin)]
#[rustio(table = "orders")]
pub struct Order {
    /// Primary key.
    pub id: i64,
    /// Company name.
    pub company: String,
    /// City.
    pub city: String,
    /// Phone number, as written.
    pub phone: String,
    /// Email address.
    pub email: String,
    /// Services, one per line, as typed. See migration 0002.
    pub services: String,
    /// Style slug from [`Style`].
    #[rustio(choices = ["klassisk", "modern", "djarv", "student", "forening", "overraska-mig"])]
    pub style: String,
    /// Package slug from [`Package`].
    #[rustio(choices = ["start", "pro"])]
    pub package: String,
    /// Where the order is in its life. The transitions are the ones
    /// [`OrderStatus`] allows.
    #[rustio(choices = ["ny", "utkast_skickat", "godkand", "publicerad", "avbruten"])]
    pub status: String,
    /// When it arrived.
    pub received_at: DateTime<Utc>,
    /// `sha256(ip || salt)`. Never the address itself.
    pub ip_hash: Option<String>,
    /// Browser string, verbatim.
    pub user_agent: Option<String>,
    /// When the studio notification went out. Empty means nobody was told.
    pub notified_at: Option<DateTime<Utc>>,
    /// The customer this order became, once accepted.
    pub customer_id: Option<i64>,
}

/// What one studio action is: a name, a label, and the code that runs it.
///
/// The `run` field is why this type exists. Holding the handler beside the
/// declaration is what stops a button existing without anything behind it.
struct OrderAction {
    name: &'static str,
    label: &'static str,
    destructive: bool,
    run: Handler,
}

/// What a bulk action's code looks like from the dispatch table.
///
/// Boxed and pinned because the handlers are `async fn` with distinct bodies,
/// and a table needs them to share one type. A plain `fn` pointer keeps the
/// entries usable in a `const`.
type Handler = for<'a> fn(
    &'a Db,
    &'a [i64],
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<BulkActionResult>> + Send + 'a>,
>;

/// The studio's workflow, in the order a job runs through it.
const ORDER_ACTIONS: &[OrderAction] = &[
    OrderAction {
        name: crate::accept::ACTION,
        label: crate::accept::LABEL,
        destructive: false,
        run: |db, ids| Box::pin(crate::accept::accept_orders(db, ids)),
    },
    OrderAction {
        name: crate::proposal::ACTION,
        label: crate::proposal::LABEL,
        destructive: false,
        run: |db, ids| Box::pin(crate::proposal::send_proposals(db, ids)),
    },
    OrderAction {
        name: crate::acceptance::ACTION,
        label: crate::acceptance::LABEL,
        destructive: false,
        run: |db, ids| Box::pin(crate::acceptance::record_acceptances(db, ids)),
    },
    OrderAction {
        name: crate::publication::ACTION,
        label: crate::publication::LABEL,
        destructive: false,
        run: |db, ids| Box::pin(crate::publication::publish_sites(db, ids)),
    },
    OrderAction {
        name: crate::cancellation::CANCEL_ACTION,
        label: crate::cancellation::CANCEL_LABEL,
        // Ends the job, and Avbruten is terminal.
        destructive: true,
        run: |db, ids| Box::pin(crate::cancellation::cancel_orders(db, ids)),
    },
    OrderAction {
        name: crate::cancellation::REFUND_ACTION,
        label: crate::cancellation::REFUND_LABEL,
        // Moves money. Not undoable from here.
        destructive: true,
        run: |db, ids| Box::pin(crate::cancellation::record_refunds(db, ids)),
    },
];

impl ModelAdmin for Order {
    fn list_display() -> &'static [&'static str] {
        &["company", "city", "package", "status", "received_at"]
    }

    /// Status first: "what is on my plate" is the question this page answers.
    fn list_filter() -> &'static [&'static str] {
        &["status", "package", "style"]
    }

    fn search_fields() -> &'static [&'static str] {
        &["company", "city", "email", "phone", "services"]
    }

    /// Newest first.
    fn ordering() -> &'static [&'static str] {
        &["-received_at"]
    }

    /// Every action the studio can run on an order.
    ///
    /// Declared and dispatched from one list on purpose. They used to be two —
    /// a `bulk_actions` array and a `match` — and they drifted: `publish_site`
    /// was declared with no arm, so the framework fell through to its default
    /// and the button reported "0 of 0" with a redirect and no trace anywhere.
    /// It looked like it worked. Sharing one entry makes that impossible.
    fn bulk_actions() -> &'static [BulkAction] {
        static DECLARED: std::sync::OnceLock<Vec<BulkAction>> = std::sync::OnceLock::new();
        DECLARED.get_or_init(|| {
            ORDER_ACTIONS
                .iter()
                .map(|a| BulkAction {
                    name: a.name,
                    label: a.label,
                    destructive: a.destructive,
                    // Every one of these emails a customer, moves money, or
                    // ends a job. None should fire on a stray click.
                    confirm: true,
                    permission: None,
                })
                .collect()
        })
    }

    fn execute_bulk_action<'a>(
        action: &'a str,
        ids: &'a [i64],
        db: &'a Db,
        _ctx: &'a BulkActionContext<'a>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<BulkActionResult>> + Send + 'a>>
    {
        Box::pin(async move {
            match ORDER_ACTIONS.iter().find(|a| a.name == action) {
                Some(entry) => (entry.run)(db, ids).await,
                // Unreachable while both sides come from ORDER_ACTIONS, and
                // loud if that ever stops being true.
                None => {
                    log::error!(
                        "bulk action {action:?} reached Order with no entry; \
                         {} row(s) left untouched",
                        ids.len()
                    );
                    Ok(BulkActionResult::default())
                }
            }
        })
    }

    /// What the customer typed, and what the request carried, are facts about
    /// the past. Editing them would quietly rewrite the record the studio is
    /// supposed to be able to trust.
    fn readonly_fields() -> &'static [&'static str] {
        &[
            "company",
            "city",
            "phone",
            "email",
            "services",
            "style",
            "package",
            "received_at",
            "ip_hash",
            "user_agent",
            "notified_at",
        ]
    }
}

/// A customer: an order that was accepted, plus the invoicing details.
#[derive(RustioAdmin)]
#[rustio(table = "customers")]
pub struct Customer {
    /// Primary key.
    pub id: i64,
    /// Company name.
    pub company: String,
    /// City.
    pub city: String,
    /// Phone number.
    pub phone: String,
    /// Email address.
    pub email: String,
    /// Swedish organisation number, once invoicing needs it.
    pub org_nr: Option<String>,
    /// Anything worth remembering about them, if anything.
    pub notes: Option<String>,
    /// When they became a customer.
    pub created_at: DateTime<Utc>,
}

impl ModelAdmin for Customer {
    fn list_display() -> &'static [&'static str] {
        &["company", "city", "email", "phone", "created_at"]
    }

    fn search_fields() -> &'static [&'static str] {
        &["company", "city", "email", "phone", "org_nr"]
    }

    fn ordering() -> &'static [&'static str] {
        &["-created_at"]
    }
}

/// A domain, registered in the customer's name.
///
/// The studio administers these but never owns them — that is the promise, and
/// `registered_to` exists so a deviation from it is something you can filter
/// for rather than something you remember.
#[derive(RustioAdmin)]
#[rustio(table = "domains")]
pub struct Domain {
    /// Primary key.
    pub id: i64,
    /// Who it belongs to.
    pub customer_id: Option<i64>,
    /// The domain itself.
    pub name: String,
    /// Where it is registered, once we know.
    pub registrar: Option<String>,
    /// Whose name the contract is in. Should always be `customer`.
    #[rustio(choices = ["customer", "studio"])]
    pub registered_to: String,
    /// When it was registered.
    pub registered_at: Option<DateTime<Utc>>,
    /// When it lapses, if nobody renews it.
    pub expires_at: Option<DateTime<Utc>>,
    /// Anything worth remembering, if anything.
    pub notes: Option<String>,
    /// When this row was made.
    pub created_at: DateTime<Utc>,
}

impl ModelAdmin for Domain {
    fn list_display() -> &'static [&'static str] {
        &["name", "registrar", "registered_to", "expires_at"]
    }

    fn list_filter() -> &'static [&'static str] {
        &["registered_to", "registrar"]
    }

    fn search_fields() -> &'static [&'static str] {
        &["name", "registrar", "notes"]
    }

    /// Soonest to expire first: this list is a renewal queue.
    fn ordering() -> &'static [&'static str] {
        &["expires_at"]
    }
}

/// The site built for a customer.
#[derive(RustioAdmin)]
#[rustio(table = "sites")]
pub struct Site {
    /// Primary key.
    pub id: i64,
    /// Who it belongs to.
    pub customer_id: Option<i64>,
    /// The order it came from.
    pub order_id: Option<i64>,
    /// The domain it answers on.
    pub domain_id: Option<i64>,
    /// The repository the customer owns, once it exists.
    pub repo_url: Option<String>,
    /// Where it is live, once it is.
    pub live_url: Option<String>,
    /// The hosting account, as the provider identifies it. Free text: every
    /// provider names an account differently.
    pub hosting_ref: Option<String>,
    /// Package slug from [`Package`].
    #[rustio(choices = ["start", "pro"])]
    pub package: String,
    /// How many revisions have been used. One is included.
    pub revisions_used: i32,
    /// When it went live.
    pub published_at: Option<DateTime<Utc>>,
    /// Anything worth remembering, if anything.
    pub notes: Option<String>,
    /// When this row was made.
    pub created_at: DateTime<Utc>,
}

impl ModelAdmin for Site {
    fn list_display() -> &'static [&'static str] {
        &["live_url", "package", "revisions_used", "published_at"]
    }

    fn list_filter() -> &'static [&'static str] {
        &["package"]
    }

    fn search_fields() -> &'static [&'static str] {
        &["repo_url", "live_url", "hosting_ref", "notes"]
    }

    fn ordering() -> &'static [&'static str] {
        &["-created_at"]
    }
}

/// The handover: the moment the promises come due.
///
/// Every column here is a promise made on the public page — the domain, the
/// hosting and the source in the customer's name, and money back before
/// publication. They are itemised because these are exactly the things that
/// get disputed later.
#[derive(RustioAdmin)]
#[rustio(table = "deliveries")]
pub struct Delivery {
    /// Primary key.
    pub id: i64,
    /// The site being handed over.
    pub site_id: Option<i64>,
    /// Who it is handed to.
    pub customer_id: Option<i64>,
    /// When the proposal was shown.
    pub offered_at: Option<DateTime<Utc>>,
    /// When the customer said yes.
    pub accepted_at: Option<DateTime<Utc>>,
    /// Domain moved into the customer's name.
    pub domain_transferred: bool,
    /// Hosting account moved into the customer's name.
    pub hosting_transferred: bool,
    /// Source code handed over.
    pub source_transferred: bool,
    /// When the money went back, if it did.
    pub refunded_at: Option<DateTime<Utc>>,
    /// Why it went back, if it did.
    pub refund_reason: Option<String>,
    /// Anything worth remembering, if anything.
    pub notes: Option<String>,
    /// When this row was made.
    pub created_at: DateTime<Utc>,
}

impl ModelAdmin for Delivery {
    fn list_display() -> &'static [&'static str] {
        &[
            "offered_at",
            "accepted_at",
            "domain_transferred",
            "source_transferred",
        ]
    }

    fn list_filter() -> &'static [&'static str] {
        &[
            "domain_transferred",
            "hosting_transferred",
            "source_transferred",
        ]
    }

    fn search_fields() -> &'static [&'static str] {
        &["refund_reason", "notes"]
    }

    fn ordering() -> &'static [&'static str] {
        &["-created_at"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hemsidor24_core::{OrderStatus, Package, Style};
    use rustio_admin::orm::Model;

    /// The `choices` lists above are hand-written strings in an attribute, so
    /// nothing but a test stops them drifting from the enums in core.
    #[test]
    fn order_choices_match_the_domain_types() {
        let styles: Vec<&str> = Style::ALL.iter().map(|s| s.slug()).collect();
        assert_eq!(
            styles,
            [
                "klassisk",
                "modern",
                "djarv",
                "student",
                "forening",
                "overraska-mig"
            ]
        );

        let packages: Vec<&str> = Package::ALL.iter().map(|p| p.slug()).collect();
        assert_eq!(packages, ["start", "pro"]);

        let statuses: Vec<&str> = OrderStatus::ALL.iter().map(|s| s.slug()).collect();
        assert_eq!(
            statuses,
            ["ny", "utkast_skickat", "godkand", "publicerad", "avbruten"]
        );
    }

    #[test]
    fn every_model_points_at_its_table() {
        assert_eq!(Order::TABLE, "orders");
        assert_eq!(Customer::TABLE, "customers");
        assert_eq!(Domain::TABLE, "domains");
        assert_eq!(Site::TABLE, "sites");
        assert_eq!(Delivery::TABLE, "deliveries");
    }

    #[test]
    fn id_is_never_inserted() {
        for columns in [
            Order::INSERT_COLUMNS,
            Customer::INSERT_COLUMNS,
            Domain::INSERT_COLUMNS,
            Site::INSERT_COLUMNS,
            Delivery::INSERT_COLUMNS,
        ] {
            assert!(!columns.contains(&"id"));
        }
    }

    #[test]
    fn the_order_list_is_newest_first_and_filters_by_status() {
        assert_eq!(Order::ordering(), &["-received_at"]);
        assert!(Order::list_filter().contains(&"status"));
    }

    #[test]
    fn what_the_customer_typed_cannot_be_edited() {
        for field in [
            "company",
            "city",
            "phone",
            "email",
            "services",
            "received_at",
        ] {
            assert!(
                Order::readonly_fields().contains(&field),
                "{field} should be read-only"
            );
        }
        // Status and customer_id are the two things the studio does change.
        assert!(!Order::readonly_fields().contains(&"status"));
        assert!(!Order::readonly_fields().contains(&"customer_id"));
    }
}

#[cfg(test)]
mod orm_contract {
    use super::*;
    use rustio_admin::Db;
    use rustio_admin::orm::Model;

    fn sample_order() -> Order {
        Order {
            id: 1,
            company: "Malmö Bygg AB".into(),
            city: "Malmö".into(),
            phone: "070-123 45 67".into(),
            email: "kontakt@malmobygg.se".into(),
            services: "Badrum".into(),
            style: "djarv".into(),
            package: "pro".into(),
            status: "ny".into(),
            received_at: rustio_admin::Utc::now(),
            ip_hash: None,
            user_agent: None,
            notified_at: None,
            customer_id: None,
        }
    }

    /// `orm::update` builds `SET col = $n` from `INSERT_COLUMNS`, then binds
    /// `insert_values()` followed by the id. If the two ever disagree the id
    /// lands in the wrong placeholder and the UPDATE silently matches nothing.
    #[test]
    fn insert_values_lines_up_with_insert_columns() {
        assert_eq!(
            Order::INSERT_COLUMNS.len(),
            sample_order().insert_values().len()
        );
    }

    #[test]
    fn columns_is_insert_columns_plus_the_id() {
        assert_eq!(Order::COLUMNS.len(), Order::INSERT_COLUMNS.len() + 1);
        assert!(Order::COLUMNS.contains(&"id"));
        assert!(!Order::INSERT_COLUMNS.contains(&"id"));
    }

    /// Guards the fix in `docs/rustio-admin-null-bind.patch`.
    ///
    /// rustio-admin 0.33.0 bound every `Value::Null` as `None::<i64>`, so a row
    /// holding NULL in any nullable non-`bigint` column could not be updated:
    ///
    /// ```text
    /// column "notified_at" is of type timestamp with time zone
    /// but expression is of type bigint
    /// ```
    ///
    /// In practice that meant the orders most worth acting on — the ones nobody
    /// was emailed about, `notified_at IS NULL` — were the ones whose status
    /// could not be changed. Skips unless `TEST_DATABASE_URL` is set.
    #[tokio::test]
    async fn a_row_with_null_timestamps_can_be_updated() {
        let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
            return;
        };
        let Ok(db) = Db::connect(&url).await else {
            return;
        };
        let Ok(orders) = rustio_admin::orm::all::<Order>(&db).await else {
            return;
        };
        let Some(mut order) = orders.into_iter().find(|o| o.notified_at.is_none()) else {
            return;
        };

        let id = order.id;
        order.status = "utkast_skickat".into();

        rustio_admin::orm::update(&db, id, &order)
            .await
            .expect("a NULL timestamp must not make a row uneditable");
    }
}
