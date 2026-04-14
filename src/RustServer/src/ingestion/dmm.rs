//! DMM hash-repo ingestion.
//!
//! Replaces the gRPC `IngestDmmPages` + .NET `DmmScraping.Execute` loop
//! with a single in-process pipeline:
//!
//!   1. `DmmRepoManager::sync_repo` clones/pulls the git repo.
//!   2. `DmmFileEntryProcessor::stream_parsed_pages` yields
//!      `ParsedDmmPageEntry` items (torrent info already enriched with an
//!      IMDb id via Tantivy fuzzy search).
//!   3. We buffer rows into batches and flush them to the `Torrents`
//!      table via [`crate::db::torrent::bulk_insert`].
//!
//! The Tantivy index and DMM repo clone are the same on-disk state the
//! gRPC path uses, so switching between the two paths is safe mid-flight.

use std::sync::Arc;

use arc_swap::ArcSwap;
use chrono::Utc;
use futures::StreamExt;
use sqlx::PgPool;

use crate::configuration::config::AppConfig;
use crate::db::torrent as torrent_repo;
use crate::dmm::db_service::PgDmmDbService;
use crate::dmm::page_parser::DmmFileEntryProcessor;
use crate::dmm::repo_manager::DmmRepoManager;
use crate::domain::torrent::TorrentInfo;
use crate::imdb::ImdbSearcher;
use crate::proto::TorrentInfo as ProtoTorrentInfo;

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
        let entry = match result {
            Ok(e) => e,
            Err(err) => {
                tracing::warn!(?err, "DMM page parse error, skipping entry");
                continue;
            }
        };

        let Some(proto) = entry.torrent_info else { continue };
        batch.push(proto_to_domain(proto));
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
    let count = torrent_repo::bulk_insert(db, &drained).await?;
    Ok(count)
}

pub struct DmmSyncReport {
    pub parsed: usize,
    pub inserted: usize,
}

/// Translate the gRPC/proto flavour of `TorrentInfo` used by the DMM page
/// parser into the `domain::TorrentInfo` the repository layer wants.
/// This is the single place where the proto↔domain conversion happens;
/// Phase 4 will inline the call once the proto module goes away.
fn proto_to_domain(p: ProtoTorrentInfo) -> TorrentInfo {
    let parsed_ingested = chrono::DateTime::parse_from_rfc3339(&p.ingested_at)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());

    TorrentInfo {
        info_hash: p.info_hash,
        raw_title: non_empty(p.raw_title),
        parsed_title: non_empty(p.parsed_title),
        normalized_title: non_empty(p.normalized_title),
        cleaned_parsed_title: p.cleaned_parsed_title,
        trash: p.trash,
        year: p.year,
        resolution: p.resolution,
        seasons: p.seasons,
        episodes: p.episodes,
        complete: p.complete,
        volumes: p.volumes,
        languages: p
            .languages
            .into_iter()
            .filter_map(|code| crate::proto::Language::try_from(code).ok())
            .map(|l| format!("{l:?}"))
            .collect(),
        quality: p
            .quality
            .and_then(|code| crate::proto::Quality::try_from(code).ok())
            .map(|q| format!("{q:?}")),
        hdr: p.hdr,
        codec: p
            .codec
            .and_then(|code| crate::proto::Codec::try_from(code).ok())
            .map(|c| format!("{c:?}")),
        audio: p.audio,
        channels: p.channels,
        dubbed: p.dubbed,
        subbed: p.subbed,
        date: p.date,
        group: p.group,
        edition: p.edition,
        bit_depth: p.bit_depth,
        bitrate: p.bitrate,
        network: p
            .network
            .and_then(|code| crate::proto::Network::try_from(code).ok())
            .map(|n| format!("{n:?}")),
        extended: p.extended,
        converted: p.convert,
        hardcoded: p.hardcoded,
        region: p.region,
        ppv: p.ppv,
        is3d: p.is_3d,
        site: p.site,
        size: p.size,
        proper: p.proper,
        repack: p.repack,
        retail: p.retail,
        upscaled: p.upscaled,
        remastered: p.remastered,
        unrated: p.unrated,
        documentary: p.documentary,
        episode_code: p.episode_code,
        country: p.country,
        container: p.container,
        extension: p.extension,
        torrent: p.torrent.unwrap_or(false),
        category: p.category,
        imdb_id: p.imdb_id,
        imdb: None,
        is_adult: p.is_adult,
        ingested_at: parsed_ingested,
    }
}

fn non_empty(s: String) -> Option<String> {
    if s.is_empty() { None } else { Some(s) }
}
