// src/main.rs — single-binary Zilean. HTTP + scheduler + in-process
// ingestion; no gRPC, no proto, no subprocess.
mod cli;
mod configuration;
mod db;
mod dmm;
mod domain;
mod http;
mod imdb;
mod ingestion;
mod scheduler;
mod torznab;
mod utils;

use std::pin::Pin;
use std::sync::Arc;

use arc_swap::ArcSwap;
use clap::Parser;
use sqlx::PgPool;
use tracing_subscriber::EnvFilter;

use crate::cli::{Cli, Command};
use crate::configuration::config::{AppConfig, load_config};
use crate::imdb::ImdbSearcher;
use crate::ingestion::generic::{Endpoint, EndpointKind};

#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let cli = Cli::parse();
    let app_config = load_config()?;

    match cli.resolved_command() {
        Command::Serve => serve(app_config).await,
        Command::DmmSync => one_shot_dmm(app_config).await,
        Command::GenericSync => one_shot_generic(app_config).await,
        Command::ResyncImdb {
            force_download,
            force_create_index,
            retag_missing_imdbs,
            retag_all_imdbs,
        } => {
            one_shot_resync_imdb(
                app_config,
                ingestion::imdb::ResyncOptions {
                    force_download,
                    force_create_index,
                    retag_missing: retag_missing_imdbs,
                    retag_all: retag_all_imdbs,
                },
            )
            .await
        }
    }
}

/// HTTP server + scheduler. Phase 4 removed the gRPC listener, so the
/// binary now runs the HTTP listener unconditionally when invoked via
/// `serve`. `ZILEAN_HTTP_PORT` defaults to 8181 (the port the .NET
/// ApiService previously used) and can be overridden.
async fn serve(app_config: AppConfig) -> anyhow::Result<()> {
    let port = app_config.http_port.unwrap_or(8181);
    let http_bind = app_config.http_bind.clone();

    let db = db::pool::connect(&app_config.database_url).await?;
    db::migrate::run(&db).await?;

    let config = Arc::new(app_config);
    let searcher = build_searcher(config.imdb_minimum_score)?;

    let sched = scheduler::Scheduler::new().await?;
    let sync_mutex = sched.sync_mutex();

    if config.dmm_scraping_enabled {
        let cfg = Arc::clone(&config);
        let pool = db.clone();
        let s = Arc::clone(&searcher);
        sched
            .schedule("DmmSync", &config.dmm_scrape_schedule, move || {
                let cfg = Arc::clone(&cfg);
                let pool = pool.clone();
                let s = Arc::clone(&s);
                let fut: Pin<
                    Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>,
                > = Box::pin(async move {
                    ingestion::dmm::run(cfg, pool, s).await?;
                    Ok(())
                });
                fut
            })
            .await?;
    }
    if config.ingestion_scraping_enabled {
        let cfg = Arc::clone(&config);
        let pool = db.clone();
        let s = Arc::clone(&searcher);
        sched
            .schedule(
                "GenericSync",
                &config.ingestion_scrape_schedule,
                move || {
                    let cfg = Arc::clone(&cfg);
                    let pool = pool.clone();
                    let s = Arc::clone(&s);
                    let fut: Pin<
                        Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>,
                    > = Box::pin(async move {
                        run_generic_all(cfg, pool, s).await?;
                        Ok(())
                    });
                    fut
                },
            )
            .await?;
    }
    sched.start().await?;

    // Startup-run-if-empty: matches .NET StartupService.StartedAsync.
    if config.dmm_scraping_enabled {
        let cfg = Arc::clone(&config);
        let pool = db.clone();
        let s = Arc::clone(&searcher);
        let m = Arc::clone(&sync_mutex);
        tokio::spawn(async move {
            if let Ok(count) =
                sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(1) FROM "ParsedPages""#)
                    .fetch_one(&pool)
                    .await
            {
                if count == 0 {
                    tracing::info!("ParsedPages empty; running DMM sync on startup");
                    let _guard = m.lock().await;
                    if let Err(err) = ingestion::dmm::run(cfg, pool, s).await {
                        tracing::error!(?err, "startup DMM sync failed");
                    }
                }
            }
        });
    }

    // HTTP handlers need the searcher + mutex to wire /dmm/on-demand-scrape.
    http::serve(
        port,
        http_bind,
        config,
        db,
        searcher,
        sync_mutex,
    )
    .await
}

async fn one_shot_dmm(app_config: AppConfig) -> anyhow::Result<()> {
    let db = db::pool::connect(&app_config.database_url).await?;
    db::migrate::run(&db).await?;
    let config = Arc::new(app_config);
    let searcher = build_searcher(config.imdb_minimum_score)?;
    ingestion::dmm::run(config, db, searcher).await.map(|_| ())
}

async fn one_shot_generic(app_config: AppConfig) -> anyhow::Result<()> {
    let db = db::pool::connect(&app_config.database_url).await?;
    db::migrate::run(&db).await?;
    let config = Arc::new(app_config);
    let searcher = build_searcher(config.imdb_minimum_score)?;
    run_generic_all(config, db, searcher).await
}

async fn one_shot_resync_imdb(
    app_config: AppConfig,
    opts: ingestion::imdb::ResyncOptions,
) -> anyhow::Result<()> {
    let db = db::pool::connect(&app_config.database_url).await?;
    db::migrate::run(&db).await?;
    let config = Arc::new(app_config);
    let searcher = build_searcher(config.imdb_minimum_score)?;
    ingestion::imdb::run(config, db, searcher, opts).await
}

fn build_searcher(min_score: f32) -> anyhow::Result<Arc<ArcSwap<ImdbSearcher>>> {
    let s = ImdbSearcher::new(min_score)?;
    Ok(Arc::new(ArcSwap::new(Arc::new(s))))
}

async fn run_generic_all(
    config: Arc<AppConfig>,
    db: PgPool,
    searcher: Arc<ArcSwap<ImdbSearcher>>,
) -> anyhow::Result<()> {
    let endpoints = parse_endpoints(config.ingestion_endpoints.as_deref());
    if endpoints.is_empty() {
        tracing::info!("no generic endpoints configured; nothing to do");
        return Ok(());
    }
    for endpoint in endpoints {
        if let Err(err) = ingestion::generic::run(
            Arc::clone(&config),
            db.clone(),
            Arc::clone(&searcher),
            endpoint.clone(),
        )
        .await
        {
            tracing::error!(?err, ?endpoint, "generic ingestion endpoint failed");
        }
    }
    Ok(())
}

fn parse_endpoints(raw: Option<&str>) -> Vec<Endpoint> {
    #[derive(serde::Deserialize)]
    struct Raw {
        kind: String,
        url: String,
        #[serde(default)]
        api_key: Option<String>,
        #[serde(default)]
        authorization: Option<String>,
        #[serde(default)]
        suffix: Option<String>,
    }

    let Some(raw) = raw.filter(|s| !s.trim().is_empty()) else {
        return Vec::new();
    };
    let parsed: Result<Vec<Raw>, _> = serde_json::from_str(raw);
    let parsed = match parsed {
        Ok(p) => p,
        Err(err) => {
            tracing::error!(?err, "failed to parse ZILEAN_INGESTION_ENDPOINTS as JSON");
            return Vec::new();
        }
    };
    parsed
        .into_iter()
        .filter_map(|r| {
            let kind = match r.kind.to_ascii_lowercase().as_str() {
                "zurg" => EndpointKind::Zurg,
                "zilean" => EndpointKind::Zilean,
                "generic" => EndpointKind::Generic,
                other => {
                    tracing::warn!(kind = other, "ignoring endpoint with unknown kind");
                    return None;
                }
            };
            Some(Endpoint {
                kind,
                url: r.url,
                api_key: r.api_key,
                authorization: r.authorization,
                suffix: r.suffix,
            })
        })
        .collect()
}
