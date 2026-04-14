//! Generic HTTP endpoint ingestion.
//!
//! Fetches a streamed JSON array of `{ name, size, hash }` items from a
//! Zurg / Zilean / Generic endpoint, parses the title via `parsett_rust`,
//! and stores the result in the `Torrents` table.
//!
//! The Phase 3 implementation is intentionally sequential and correctness-
//! first: it de-duplicates against existing hashes, runs `parse_batch` on
//! the survivors, and funnels everything into
//! [`crate::db::torrent::bulk_insert`]. A follow-up can introduce streaming
//! (rayon batched) parsing for throughput, matching the .NET
//! `StreamedEntryProcessor` pipeline.

use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use chrono::Utc;
use reqwest::header;
use serde::Deserialize;
use sqlx::PgPool;

use crate::configuration::config::AppConfig;
use crate::db::torrent as torrent_repo;
use crate::domain::torrent::TorrentInfo;
use crate::imdb::ImdbSearcher;
use crate::utils::query::clean_query;
use crate::utils::strings::normalize_title;
use parsett_rust::parse_batch;

/// Endpoint the ingestion should pull from.
#[derive(Debug, Clone)]
pub struct Endpoint {
    pub kind: EndpointKind,
    pub url: String,
    pub api_key: Option<String>,
    pub authorization: Option<String>,
    pub suffix: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointKind {
    Zurg,
    Zilean,
    Generic,
}

impl Endpoint {
    fn full_url(&self, cfg: &AppConfig) -> String {
        let _ = cfg; // reserved for future use when suffix defaults move here
        match self.kind {
            EndpointKind::Zurg => format!("{}{}", self.url, "/debug/torrents"),
            EndpointKind::Zilean => format!("{}{}", self.url, "/torrents/all"),
            EndpointKind::Generic => {
                let suffix = self.suffix.as_deref().unwrap_or("");
                format!("{}{}", self.url, suffix)
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct RawEntry {
    #[serde(default)]
    name: String,
    #[serde(default)]
    size: i64,
    #[serde(default)]
    hash: String,
}

/// Run one ingestion pass over a single endpoint. Logs a per-endpoint
/// summary at info level and returns a count of newly stored torrents.
pub async fn run(
    config: Arc<AppConfig>,
    db: PgPool,
    searcher: Arc<ArcSwap<ImdbSearcher>>,
    endpoint: Endpoint,
) -> anyhow::Result<usize> {
    let url = endpoint.full_url(&config);
    tracing::info!(kind = ?endpoint.kind, %url, "generic ingestion starting");

    // Request stage.
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(10_000))
        .build()?;

    let mut req = client.get(&url);
    if endpoint.kind == EndpointKind::Zilean {
        if let Some(key) = endpoint.api_key.as_deref() {
            req = req.header("X-Api-Key", key);
        }
    }
    if endpoint.kind == EndpointKind::Generic {
        if let Some(auth) = endpoint.authorization.as_deref() {
            req = req.header(header::AUTHORIZATION, auth);
        }
    }

    let response = req.send().await?.error_for_status()?;
    let bytes = response.bytes().await?;
    let raw_entries: Vec<RawEntry> = serde_json::from_slice(&bytes)?;

    tracing::info!(%url, count = raw_entries.len(), "generic ingestion downloaded");

    // Deduplicate against the Torrents table so we only bother parsing
    // novel hashes. Mirrors what `StreamedEntryProcessor` does on the
    // .NET side.
    let incoming_hashes: Vec<String> =
        raw_entries.iter().map(|e| e.hash.clone()).collect();
    let existing = torrent_repo::existing_hashes(&db, &incoming_hashes).await?;
    let novel: Vec<RawEntry> = raw_entries
        .into_iter()
        .filter(|e| e.hash.len() == 40 && !existing.contains(&e.hash))
        .collect();

    if novel.is_empty() {
        tracing::info!(%url, "generic ingestion: no new torrents");
        return Ok(0);
    }

    // Parse titles in parallel (parsett_rust uses rayon under the hood).
    let titles: Vec<&str> = novel.iter().map(|e| e.name.as_str()).collect();
    let parsed = parse_batch(titles);

    // Build the TorrentInfo rows in lockstep with the parsed results.
    let mut rows: Vec<TorrentInfo> = Vec::with_capacity(novel.len());
    for (entry, parse_result) in novel.into_iter().zip(parsed) {
        let parsed = match parse_result {
            Ok(p) => p,
            Err(err) => {
                tracing::warn!(?err, title = %entry.name, "failed to parse title");
                continue;
            }
        };

        let normalized = normalize_title(&parsed.title);
        let category = assign_category(parsed.adult, &parsed.seasons, &parsed.episodes);
        let cleaned = clean_query(&parsed.title);

        // Best-effort IMDb enrichment.
        let imdb_id = searcher
            .load()
            .search(&normalized, &category, parsed.year.unwrap_or(0))
            .into_iter()
            .next()
            .map(|m| m.imdb_id);

        rows.push(TorrentInfo {
            info_hash: entry.hash,
            raw_title: Some(entry.name),
            parsed_title: Some(parsed.title.clone()),
            normalized_title: Some(normalized),
            cleaned_parsed_title: Some(cleaned),
            trash: parsed.trash,
            year: parsed.year,
            resolution: parsed.resolution,
            seasons: parsed.seasons,
            episodes: parsed.episodes,
            complete: parsed.complete,
            volumes: parsed.volumes,
            languages: parsed.languages.into_iter().map(|l| format!("{l:?}")).collect(),
            quality: parsed.quality.map(|q| format!("{q:?}")),
            hdr: parsed.hdr,
            codec: parsed.codec.map(|c| format!("{c:?}")),
            audio: parsed.audio,
            channels: parsed.channels,
            dubbed: parsed.dubbed,
            subbed: parsed.subbed,
            date: parsed.date,
            group: parsed.group,
            edition: parsed.edition,
            bit_depth: parsed.bit_depth,
            bitrate: parsed.bitrate,
            network: parsed.network.map(|n| format!("{n:?}")),
            extended: parsed.extended,
            converted: parsed.convert,
            hardcoded: parsed.hardcoded,
            region: parsed.region,
            ppv: parsed.ppv,
            is3d: parsed.is_3d,
            site: parsed.site,
            size: Some(entry.size.to_string()),
            proper: parsed.proper,
            repack: parsed.repack,
            retail: parsed.retail,
            upscaled: parsed.upscaled,
            remastered: parsed.remastered,
            unrated: parsed.unrated,
            documentary: parsed.documentary,
            episode_code: parsed.episode_code,
            country: None,
            container: parsed.container,
            extension: parsed.extension,
            torrent: false,
            category,
            imdb_id,
            imdb: None,
            is_adult: parsed.adult,
            ingested_at: Utc::now(),
        });
    }

    let inserted = torrent_repo::bulk_insert(&db, &rows).await?;
    tracing::info!(%url, inserted, "generic ingestion finished");

    Ok(inserted)
}

fn assign_category(adult: bool, seasons: &[i32], episodes: &[i32]) -> String {
    if adult {
        "xxx".into()
    } else if seasons.is_empty() && episodes.is_empty() {
        "movie".into()
    } else {
        "tvSeries".into()
    }
}
