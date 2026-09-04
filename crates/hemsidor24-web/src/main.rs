//! Hemsidor24 public web server.

mod config;
mod form;
mod orders;
mod routes;
mod spam;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::extract::Request;
use hemsidor24_notify::{FakeNotifier, Notifier, SmtpNotifier};
use sqlx::postgres::PgPoolOptions;
use thiserror::Error;
use tokio::net::TcpListener;
use tokio::signal;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use crate::config::{Config, ConfigError};
use crate::routes::AppState;

/// Source of per-request ids.
///
/// A counter rather than a UUID: it is enough to tie log lines from one request
/// together, and it avoids pulling in a dependency to generate random numbers.
static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// Anything that can stop the server from starting.
#[derive(Debug, Error)]
enum StartupError {
    /// The environment said something unusable.
    #[error(transparent)]
    Config(#[from] ConfigError),
    /// Postgres could not be reached, or the migrations would not apply.
    #[error("database is not usable")]
    Database(#[from] sqlx::Error),
    /// A migration failed.
    #[error("could not apply migrations")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    /// The rate limiter was configured with impossible numbers.
    #[error("invalid rate limit configuration")]
    RateLimit,
    /// The address is taken, or not ours to bind.
    #[error("could not bind to {addr}")]
    Bind {
        /// The address we tried.
        addr: std::net::SocketAddr,
        /// Why the OS refused.
        #[source]
        source: std::io::Error,
    },
    /// The server stopped with an error rather than a shutdown signal.
    #[error("server stopped unexpectedly")]
    Serve(#[source] std::io::Error),
}

#[tokio::main]
async fn main() -> Result<(), StartupError> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("hemsidor24_web=info,hemsidor24_notify=info,tower_http=info")
    });
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let config = Config::from_env()?;

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await?;
    sqlx::migrate!("../../migrations").run(&pool).await?;
    tracing::info!("migrations up to date");

    match config.smtp.clone() {
        Some(smtp) => {
            let notifier = SmtpNotifier::connect(&smtp).map_err(|error| {
                tracing::error!(%error, "SMTP settings are unusable");
                StartupError::RateLimit
            })?;
            tracing::info!(host = %smtp.host, port = smtp.port, to = %smtp.notify_to, "SMTP ready");
            serve(config, pool, notifier, smtp.notify_to, smtp.notify_from).await
        }
        None => {
            // Deliberately not fatal. Orders still reach Postgres and the admin
            // panel, which is what must never break. But this is the kind of
            // thing you want to notice in a log.
            tracing::error!(
                "NO SMTP CONFIGURED — orders will be stored but nobody will be emailed. \
                 Set SMTP_HOST, SMTP_USER, SMTP_PASSWORD, NOTIFY_TO and NOTIFY_FROM."
            );
            let to = "hej@hemsidor24.se".to_owned();
            serve(config, pool, FakeNotifier::new(), to.clone(), to).await
        }
    }
}

/// Build the router and run until a shutdown signal.
async fn serve<N: Notifier + Send + Sync + 'static>(
    config: Config,
    pool: sqlx::PgPool,
    notifier: N,
    notify_to: String,
    notify_from: String,
) -> Result<(), StartupError> {
    let state = Arc::new(AppState {
        pool,
        notifier,
        ip_hash_salt: config.ip_hash_salt.clone(),
        notify_to,
        notify_from,
    });

    let app = routes::router(state)
        .map_err(|_| StartupError::RateLimit)?
        .layer(
            TraceLayer::new_for_http().make_span_with(|request: &Request| {
                tracing::info_span!(
                    "request",
                    id = REQUEST_ID.fetch_add(1, Ordering::Relaxed),
                    method = %request.method(),
                    path = %request.uri().path(),
                )
            }),
        );

    let listener = TcpListener::bind(config.addr)
        .await
        .map_err(|source| StartupError::Bind {
            addr: config.addr,
            source,
        })?;

    tracing::info!(addr = %config.addr, "hemsidor24-web listening");

    // into_make_service_with_connect_info so the rate limiter and the IP hash
    // can see who is calling.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .map_err(StartupError::Serve)?;

    tracing::info!("shutdown complete");
    Ok(())
}

/// Resolves on Ctrl-C or on SIGTERM, whichever arrives first.
async fn shutdown_signal() {
    let ctrl_c = async {
        match signal::ctrl_c().await {
            Ok(()) => tracing::info!("SIGINT received, shutting down"),
            Err(error) => tracing::error!(%error, "could not listen for SIGINT"),
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
                tracing::info!("SIGTERM received, shutting down");
            }
            Err(error) => {
                tracing::error!(%error, "could not listen for SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
}
