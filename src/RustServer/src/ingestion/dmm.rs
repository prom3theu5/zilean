//! DMM hash-repo ingestion.
//!
//! 1. [`DmmRepoManager::sync_repo`] clones/pulls the git repo.
//! 2. [`DmmFileEntryProcessor::stream_parsed_pages`] yields
//!    [`TorrentInfo`] items (torrent info already enriched with an
//!    IMDb id via Tantivy fuzzy search).
//! 3. We buffer rows into batches and flush them to the `Torrents`
//!    table via [`crate::db::torrent::bulk_insert`].

use std::sync::Arc;

use arc_swap::ArcSwap;
use futures::StreamExt;
use sqlx::PgPool;

use crate::configuration::config::AppConfig;
use crate::db::torrent as torrent_repo;
use crate::dmm::db_service::PgDmmDbService;
use crate::dmm::page_parser::DmmFileEntryProcessor;
use crate::dmm::repo_manager::DmmRepoManager;
use crate::domain::torrent::TorrentInfo;
use crate::imdb::ImdbSearcher;

/// Run a single DMM sync. Vacuums the `Torrents` table when finished so
/// the trigram index statistics stay fresh.
pub async fn run(
    config: Arc<AppConfig>,
    db: PgPool,
    searcher: Arc<ArcSwap<ImdbSearcher>>,
) -> anyhow::Result<DmmSyncReport> {
    tracing::info!("DMM sync starting");

    let repo = DmmRepoManager::new(&config.dmm_repo_url, &config.dmm_local_path);
    repo.sync_repo()?;

    let dmm_service = Arc::new(PgDmmDbService::connect(&config.database_url).await?);
    let processor = Arc::new(DmmFileEntryProcessor::new(
        dmm_service,
        searcher,
        config.dmm_local_path.clone(),
    ));

    const BATCH: usize = 5000;
    let mut batch: Vec<TorrentInfo> = Vec::with_capacity(BATCH);
    let mut total_parsed = 0usize;
    let mut total_inserted = 0usize;

    let stream = processor.clone().stream_parsed_pages();
    tokio::pin!(stream);

    while let Some(result) = stream.next().await {
        let torrent = match result {
            Ok(t) => t,
            Err(err) => {
                tracing::warn!(?err, "DMM page parse error, skipping entry");
                continue;
            }
        };

        batch.push(torrent);
        total_parsed += 1;

        if batch.len() >= BATCH {
            total_inserted += flush(&db, &mut batch).await?;
        }
    }

    if !batch.is_empty() {
        total_inserted += flush(&db, &mut batch).await?;
    }

    tracing::info!(
        "DMM sync: parsed {total_parsed} torrents, inserted {total_inserted} new rows"
    );

    if let Err(err) = torrent_repo::vacuum_analyze(&db).await {
        tracing::warn!(?err, "post-ingestion VACUUM ANALYZE failed");
    }

    Ok(DmmSyncReport {
        parsed: total_parsed,
        inserted: total_inserted,
    })
}

async fn flush(db: &PgPool, batch: &mut Vec<TorrentInfo>) -> anyhow::Result<usize> {
    let drained: Vec<TorrentInfo> = std::mem::take(batch);
    torrent_repo::bulk_insert(db, &drained).await
}

pub struct DmmSyncReport {
    pub parsed: usize,
    pub inserted: usize,
}
