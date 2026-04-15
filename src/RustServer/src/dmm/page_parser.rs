//! DMM page parser.
//!
//! Walks the cloned DMM hash repository, decompresses the LZ-string JSON
//! embedded in each HTML page, and yields one [`TorrentInfo`] per torrent
//! found. Phase 5 deleted the proto hop that used to sit between the
//! parser and the JSON response layer; this module now produces the
//! canonical `domain::TorrentInfo` directly.

use crate::dmm::db_service::DmmDbService;
use crate::dmm::types::parsed_pages::ParsedPages;
use crate::domain::torrent::TorrentInfo;
use crate::imdb::searcher::ImdbSearcher;
use arc_swap::ArcSwap;
use async_stream::try_stream;
use dashmap::DashMap;
use futures::StreamExt;
use futures_core::stream::Stream;
use lazy_static::lazy_static;
use parsett_rust::parse_title;
use rayon::prelude::*;
use regex::Regex;
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use tokio::fs::{read_dir, read_to_string};
use tracing::log::warn;
use tracing::{debug, error, info};

pub struct DmmFileEntryProcessor {
    dmm_service: Arc<dyn DmmDbService + Send + Sync>,
    imdb_searcher: Arc<ArcSwap<ImdbSearcher>>,
    repo_root: String,
    existing_pages: DashMap<String, i32>,
    new_pages: DashMap<String, i32>,
}

lazy_static! {
    static ref HASH_IFRAME_REGEX: Regex =
        Regex::new(r#"<iframe\s+src="https://debridmediamanager\.com/hashlist#([^"]+)"[^>]*>"#)
            .expect("Failed to compile Iframe Regex");
}

impl DmmFileEntryProcessor {
    pub fn new(
        dmm_service: Arc<dyn DmmDbService + Send + Sync>,
        imdb_searcher: Arc<ArcSwap<ImdbSearcher>>,
        repo_root: String,
    ) -> Self {
        Self {
            dmm_service,
            imdb_searcher,
            repo_root,
            existing_pages: DashMap::new(),
            new_pages: DashMap::new(),
        }
    }

    /// Produce a [`Stream`] of fully populated [`TorrentInfo`] rows, ready
    /// to be bulk-inserted into the `Torrents` table.
    ///
    /// The stream walks every `.html` file in the DMM repo (excluding
    /// `index.html`), skipping any file already recorded in
    /// `ParsedPages`. Pages that yielded at least one torrent are recorded
    /// atomically via [`Self::add_parsed_page`] so a subsequent run picks
    /// up where the previous one left off.
    pub fn stream_parsed_pages(
        self: Arc<Self>,
    ) -> impl Stream<Item = Result<TorrentInfo, anyhow::Error>> + Send + 'static {
        try_stream! {
            let filenames = match self.get_pages_from_repo_root().await {
                Ok(f) => f,
                Err(e) => {
                    error!(?e, "Failed to list repo root");
                    return;
                }
            };

            self.load_parsed_pages().await?;

            for file in &filenames {
                let filename = Path::new(file)
                    .file_name()
                    .and_then(|f| f.to_str())
                    .unwrap_or_default()
                    .to_string();

                if self.existing_pages.contains_key(&filename) || self.new_pages.contains_key(&filename) {
                    continue;
                }

                let mut count = 0;
                let stream = self.clone().process_page_stream(file.clone(), filename.clone());
                tokio::pin!(stream);

                while let Some(result) = stream.next().await {
                    match result {
                        Ok(torrent) => {
                            yield torrent;
                            count += 1;
                        }
                        Err(err) => {
                            error!(?err, "Error processing entry for {filename}");
                        }
                    }
                }

                self.add_parsed_page(&filename, count).await?;
            }
        }
    }

    fn process_page_stream(
        self: Arc<Self>,
        file: String,
        filename_only: String,
    ) -> impl Stream<Item = Result<TorrentInfo, anyhow::Error>> + Send + 'static {
        let this = Arc::clone(&self);

        try_stream! {
            if !Path::new(&file).exists() {
                return;
            }

            info!("Processing file: {}", filename_only);

            let page_source = read_to_string(&file).await?;
            let Some(caps) = HASH_IFRAME_REGEX.captures(&page_source) else {
                debug!("No hash data found in file: {}", filename_only);
                return;
            };

            let hash_data = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
            let decompressed_data =
                lz_str::decompress_from_encoded_uri_component(hash_data).expect("invalid hash data");
            let json_str = String::from_utf16(&decompressed_data).expect("invalid UTF16");

            let json: Value = serde_json::from_str(&json_str)?;
            let torrents = match &json {
                Value::Array(arr) => arr.clone(),
                Value::Object(map) => match map.get("torrents") {
                    Some(Value::Array(arr)) => arr.clone(),
                    _ => return,
                },
                _ => return,
            };

            // Parallel parsing is where this pipeline spends the vast
            // majority of its CPU. `parsett_rust` is CPU-bound and lock-
            // free, so Rayon scales close to linearly with `parsing_threads`.
            let results: Vec<TorrentInfo> = torrents
                .into_par_iter()
                .filter_map(|item| this.process_torrent_item(item))
                .collect();

            for torrent_info in results {
                yield torrent_info;
            }
        }
    }

    fn process_torrent_item(&self, item: Value) -> Option<TorrentInfo> {
        let title = match item.get("filename").and_then(|t| t.as_str()) {
            Some(t) => t,
            None => {
                warn!("Skipping item: missing or invalid 'filename'");
                return None;
            }
        };

        let info_hash = match item.get("hash").and_then(|h| h.as_str()) {
            Some(h) => h,
            None => {
                warn!("Skipping item '{}': missing or invalid 'hash'", title);
                return None;
            }
        };

        let bytes = match item.get("bytes").and_then(|b| b.as_i64()) {
            Some(b) => b,
            None => {
                warn!("Skipping item '{}': missing or invalid 'bytes'", title);
                return None;
            }
        };

        let parsed = match parse_title(title) {
            Ok(p) => p,
            Err(e) => {
                warn!("Skipping item '{}': failed to parse title: {:?}", title, e);
                return None;
            }
        };

        let mut torrent_info = TorrentInfo::from_parsed_title(
            info_hash.to_owned(),
            title.to_owned(),
            bytes,
            parsed,
        );

        // Best-effort IMDb enrichment. `search` needs the normalised
        // title + category + year, all of which `from_parsed_title`
        // already populated.
        if let Some(normalized) = torrent_info.normalized_title.as_deref() {
            let year = torrent_info.year.unwrap_or_default();
            if let Some(best) = self
                .imdb_searcher
                .load()
                .search(normalized, &torrent_info.category, year)
                .into_iter()
                .next()
            {
                torrent_info.imdb_id = Some(best.imdb_id);
            }
        }

        Some(torrent_info)
    }

    async fn load_parsed_pages(&self) -> anyhow::Result<()> {
        let parsed = self.dmm_service.get_ingested_pages().await?;
        for page in parsed {
            self.existing_pages.insert(page.page, page.entry_count);
        }
        Ok(())
    }

    async fn add_parsed_page(&self, filename: &str, entry_count: i32) -> anyhow::Result<()> {
        self.dmm_service
            .add_page_to_ingested(&ParsedPages {
                page: filename.to_string(),
                entry_count,
            })
            .await?;
        self.new_pages.insert(filename.to_string(), entry_count);
        Ok(())
    }

    async fn get_pages_from_repo_root(&self) -> anyhow::Result<Vec<String>> {
        let mut pages = Vec::new();
        if !Path::new(&self.repo_root).exists() {
            return Ok(pages);
        }
        let mut dir = read_dir(&self.repo_root).await?;
        while let Some(entry) = dir.next_entry().await? {
            if entry.file_type().await?.is_file() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with(".html") && name != "index.html" {
                        pages.push(path.to_string_lossy().into_owned());
                    }
                }
            }
        }

        debug!("Found {} DMM HTML files in repo", pages.len());

        Ok(pages)
    }
}
