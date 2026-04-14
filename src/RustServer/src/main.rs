// src/main.rs
mod configuration;
mod db;
mod dmm;
mod domain;
mod grpc;
mod http;
mod imdb;
mod torznab;
mod utils;

use std::sync::Arc;

use crate::configuration::config::load_config;

pub mod proto {
    tonic::include_proto!("zilean_rust");
}

use grpc::server::start_server as start_grpc_server;
use tracing_subscriber::EnvFilter;

#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let app_config = load_config()?;

    // Phase 2: when ZILEAN_HTTP_PORT is set, spin up the axum listener in
    // parallel with the existing gRPC server. When unset, behaviour is
    // exactly what it was before Phase 2 (gRPC-only, Unix socket).
    let http_port = app_config.http_port;
    let http_bind = app_config.http_bind.clone();

    if let Some(port) = http_port {
        // Run migrations once, up front, on a small dedicated pool that
        // the HTTP layer then reuses. Running migrations before we begin
        // accepting HTTP/gRPC traffic means the first real request never
        // sees a half-applied schema.
        let db = db::pool::connect(&app_config.database_url).await?;
        db::migrate::run(&db).await?;

        let config = Arc::new(app_config);
        let http_task = tokio::spawn(http::serve(port, http_bind, config.clone(), db.clone()));

        let grpc_config = (*config).clone();
        let grpc_task = tokio::spawn(start_grpc_server(grpc_config));

        // Whichever task terminates first dictates the process exit code.
        // Under normal SIGTERM-driven shutdown the HTTP task exits via the
        // graceful-shutdown signal handler first; the gRPC server follows
        // when the same runtime shuts down. An error from either side is
        // bubbled out so systemd/k8s can retry with backoff.
        tokio::select! {
            r = http_task => r??,
            r = grpc_task => r??,
        }
        Ok(())
    } else {
        // HTTP disabled: retain pre-Phase-2 behaviour exactly. This is the
        // production default until operators opt in.
        start_grpc_server(app_config).await
    }
}

