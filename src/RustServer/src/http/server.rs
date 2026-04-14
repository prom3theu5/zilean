//! HTTP server bootstrap.
//!
//! Wires the per-feature routers into a single [`axum::Router`], installs
//! cross-cutting tower-http layers (tracing, request timeout), binds a
//! TCP listener, and runs `axum::serve` until the process is asked to
//! shut down.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use axum::Router;
use tokio::net::TcpListener;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::configuration::config::AppConfig;

use super::routes;
use super::state::AppState;

/// Build the axum router for a given [`AppState`]. Each route group is
/// guarded by its own feature toggle, matching the .NET `Enable*`
/// configuration.
pub fn build_router(state: AppState) -> Router {
    let mut app: Router<AppState> = Router::new();

    // Health check is always on.
    app = app.merge(routes::healthchecks::router());

    if state.config.dmm_enabled {
        app = app.merge(routes::dmm::router(state.clone()));
    }
    if state.config.imdb_enabled {
        app = app.merge(routes::imdb::router());
    }
    if state.config.torznab_enabled {
        app = app.merge(routes::torznab::router());
    }
    if state.config.torrents_scrape_enabled || state.config.torrents_cache_check_enabled {
        app = app.merge(routes::torrents::router(state.clone()));
    }

    // Blacklist endpoints are always registered when HTTP is on. The .NET
    // version does not gate them behind a config flag.
    app = app.merge(routes::blacklist::router(state.clone()));

    app.with_state(state)
        // 5 minute cap for very long search queries; streaming responses
        // set their own longer budgets via axum's streaming layer.
        .layer(TimeoutLayer::new(Duration::from_secs(300)))
        .layer(TraceLayer::new_for_http())
}

/// Bind and run the HTTP server. Returns only when the listener terminates
/// (which in practice means a SIGTERM or SIGINT was received, because
/// axum's `serve` honours the runtime's shutdown signal handling).
///
/// `bind` is taken by value so the resulting future is `'static`, which is
/// what `tokio::spawn` requires.
pub async fn serve(
    port: u16,
    bind: String,
    config: Arc<AppConfig>,
    db: sqlx::PgPool,
) -> anyhow::Result<()> {
    let addr: SocketAddr = format!("{bind}:{port}")
        .parse()
        .with_context(|| format!("parsing HTTP bind address '{bind}:{port}'"))?;

    let state = AppState::new(db, config);
    let app = build_router(state);

    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding HTTP listener on {addr}"))?;
    tracing::info!("HTTP listener ready on {addr}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("axum::serve terminated with an error")?;

    Ok(())
}

async fn shutdown_signal() {
    use tokio::signal;

    let ctrl_c = async {
        let _ = signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        let mut term = signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler");
        term.recv().await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => { tracing::info!("HTTP server: SIGINT received, shutting down"); }
        _ = terminate => { tracing::info!("HTTP server: SIGTERM received, shutting down"); }
    }
}
