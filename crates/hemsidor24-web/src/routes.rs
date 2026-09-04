//! The routes the public site serves.

use std::sync::Arc;

use askama::Template;
use axum::Router;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use hemsidor24_notify::Notifier;
use sqlx::PgPool;
use tower_governor::GovernorLayer;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::SmartIpKeyExtractor;
use tower_http::services::ServeDir;

use crate::form::{FormView, OrderSubmission};
use crate::orders::{self, RequestMeta};
use crate::spam;

/// Static assets live next to the crate, so `cargo run` works from anywhere in
/// the workspace. Deployment will want a configurable path; that is a phase 6
/// problem, not a phase 3 one.
const STATIC_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/static");

/// How many orders one IP may post before being turned away.
const RATE_LIMIT_BURST: u32 = 5;

/// Seconds between replenished attempts.
const RATE_LIMIT_PER_SECONDS: u64 = 60;

/// The rate-limit constants above do not describe a usable limiter.
///
/// Only reachable by editing them to something impossible, so it carries no
/// detail beyond saying so.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("rate limit configuration is impossible")]
pub struct RateLimitConfigError;

/// What every handler needs.
pub struct AppState<N: Notifier> {
    /// The source of truth.
    pub pool: PgPool,
    /// How mail goes out.
    pub notifier: N,
    /// Salt for the stored IP hash.
    pub ip_hash_salt: String,
    /// Where studio notifications go.
    pub notify_to: String,
    /// Envelope sender.
    pub notify_from: String,
}

/// The public landing page.
#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    /// The order form, blank or repopulated after a rejection.
    form: FormView,
    /// Whether to show the confirmation instead of a fresh form.
    sent: bool,
}

/// Wraps a template so a render failure becomes a 500 instead of a panic.
struct Page<T>(StatusCode, T);

impl<T: Template> IntoResponse for Page<T> {
    fn into_response(self) -> Response {
        match self.1.render() {
            Ok(html) => (self.0, Html(html)).into_response(),
            Err(error) => {
                tracing::error!(%error, "template failed to render");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "500 Internal Server Error",
                )
                    .into_response()
            }
        }
    }
}

/// `GET /`
async fn index() -> impl IntoResponse {
    Page(
        StatusCode::OK,
        IndexTemplate {
            form: FormView::default(),
            sent: false,
        },
    )
}

/// `GET /health` — plain text, no template, nothing that can fail.
async fn health() -> &'static str {
    "ok"
}

/// `POST /bestall`
///
/// Validate, store, then notify — in that order, always. See [`crate::orders`]
/// for why the order matters.
async fn submit_order<N: Notifier + Send + Sync + 'static>(
    State(state): State<Arc<AppState<N>>>,
    ConnectInfo(peer): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    axum::Form(submission): axum::Form<OrderSubmission>,
) -> Response {
    // Spam first: no point validating, storing or emailing a bot's typing.
    // A caught bot gets the same confirmation a customer does.
    let verdict = spam::check(&submission.webbplats, &submission.oppnad);
    if verdict.is_spam() {
        tracing::warn!(?verdict, "submission rejected as spam");
        return Page(
            StatusCode::OK,
            IndexTemplate {
                form: FormView::default(),
                sent: true,
            },
        )
        .into_response();
    }

    let order = match submission.to_core().validate() {
        Ok(order) => order,
        Err(errors) => {
            tracing::info!(fields = errors.len(), "order form rejected");
            return Page(
                StatusCode::UNPROCESSABLE_ENTITY,
                IndexTemplate {
                    form: FormView::from_rejected(&submission, &errors),
                    sent: false,
                },
            )
            .into_response();
        }
    };

    let meta = RequestMeta {
        ip: Some(peer.ip().to_string()),
        user_agent: headers
            .get(axum::http::header::USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned),
    };

    // The only failure the customer is allowed to see.
    let order_id = match orders::store(&state.pool, &order, &meta, &state.ip_hash_salt).await {
        Ok(id) => id,
        Err(error) => {
            tracing::error!(%error, "could not store an order — the customer was told to retry");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Något gick fel när beställningen skulle sparas. Försök igen, \
                 eller mejla hej@hemsidor24.se.",
            )
                .into_response();
        }
    };

    tracing::info!(order_id, package = order.package().slug(), "order stored");

    // Past this point the order is safe, so nothing below may fail the request.
    orders::notify(
        &state.pool,
        &state.notifier,
        &order,
        order_id,
        &state.notify_to,
        &state.notify_from,
    )
    .await;

    Page(
        StatusCode::OK,
        IndexTemplate {
            form: FormView::default(),
            sent: true,
        },
    )
    .into_response()
}

/// Every route the server answers.
///
/// # Errors
///
/// Returns [`RateLimitConfigError`] if the rate-limit constants above are
/// impossible, which can only happen if someone edits them to something silly.
pub fn router<N: Notifier + Send + Sync + 'static>(
    state: Arc<AppState<N>>,
) -> Result<Router, RateLimitConfigError> {
    let governor = GovernorConfigBuilder::default()
        .per_second(RATE_LIMIT_PER_SECONDS)
        .burst_size(RATE_LIMIT_BURST)
        .key_extractor(SmartIpKeyExtractor)
        .finish()
        .ok_or(RateLimitConfigError)?;

    // The rate limit guards the write endpoint only. Nobody should be throttled
    // out of reading a marketing page.
    let bestall = Router::new()
        .route("/bestall", post(submit_order::<N>))
        .layer(GovernorLayer {
            config: Arc::new(governor),
        })
        .with_state(state.clone());

    Ok(Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .nest_service("/static", ServeDir::new(STATIC_DIR))
        .merge(bestall)
        .with_state(state))
}
