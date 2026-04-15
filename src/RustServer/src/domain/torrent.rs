//! TorrentInfo and friends: the fat response DTO for every search endpoint,
//! plus the filter shapes consumed by the handlers.

use chrono::{DateTime, Utc};
use parsett_rust::ParsedTitle;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use sqlx::postgres::PgRow;

use super::imdb::ImdbFile;

/// A single torrent record as returned by every public search endpoint.
///
/// Field names in the generated JSON match the .NET `TorrentInfo` class
/// byte-for-byte. Specifically:
///
/// * Most fields serialise as snake_case (that class has explicit
///   `[JsonPropertyName]` attributes producing snake_case regardless of
///   ASP.NET's default camelCase policy).
/// * `Is3d` is exposed on the wire as `_3d`.
/// * `IsAdult` is exposed as `adult`.
/// * The nested `imdb` field is an [`ImdbFile`] object (or `null`).
///
/// Boolean fields the database constrains to `NOT NULL` are modelled as
/// `bool` in Rust. Fields that the database allows to be `NULL` are
/// `Option<T>`. Arrays are `NOT NULL` in the schema, so `Vec<T>` is used.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TorrentInfo {
    #[serde(rename = "raw_title", skip_serializing_if = "Option::is_none")]
    pub raw_title: Option<String>,
    #[serde(rename = "parsed_title", skip_serializing_if = "Option::is_none")]
    pub parsed_title: Option<String>,
    #[serde(rename = "normalized_title", skip_serializing_if = "Option::is_none")]
    pub normalized_title: Option<String>,
    #[serde(rename = "cleaned_parsed_title", skip_serializing_if = "Option::is_none")]
    pub cleaned_parsed_title: Option<String>,
    #[serde(rename = "trash")]
    pub trash: bool,
    #[serde(rename = "year", skip_serializing_if = "Option::is_none")]
    pub year: Option<i32>,
    #[serde(rename = "resolution", skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
    #[serde(rename = "seasons")]
    pub seasons: Vec<i32>,
    #[serde(rename = "episodes")]
    pub episodes: Vec<i32>,
    #[serde(rename = "complete")]
    pub complete: bool,
    #[serde(rename = "volumes")]
    pub volumes: Vec<i32>,
    #[serde(rename = "languages")]
    pub languages: Vec<String>,
    #[serde(rename = "quality", skip_serializing_if = "Option::is_none")]
    pub quality: Option<String>,
    #[serde(rename = "hdr")]
    pub hdr: Vec<String>,
    #[serde(rename = "codec", skip_serializing_if = "Option::is_none")]
    pub codec: Option<String>,
    #[serde(rename = "audio")]
    pub audio: Vec<String>,
    #[serde(rename = "channels")]
    pub channels: Vec<String>,
    #[serde(rename = "dubbed")]
    pub dubbed: bool,
    #[serde(rename = "subbed")]
    pub subbed: bool,
    #[serde(rename = "date", skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(rename = "group", skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(rename = "edition", skip_serializing_if = "Option::is_none")]
    pub edition: Option<String>,
    #[serde(rename = "bit_depth", skip_serializing_if = "Option::is_none")]
    pub bit_depth: Option<String>,
    #[serde(rename = "bitrate", skip_serializing_if = "Option::is_none")]
    pub bitrate: Option<String>,
    #[serde(rename = "network", skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,
    #[serde(rename = "extended")]
    pub extended: bool,
    #[serde(rename = "converted")]
    pub converted: bool,
    #[serde(rename = "hardcoded")]
    pub hardcoded: bool,
    #[serde(rename = "region", skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(rename = "ppv")]
    pub ppv: bool,
    #[serde(rename = "_3d")]
    pub is3d: bool,
    #[serde(rename = "site", skip_serializing_if = "Option::is_none")]
    pub site: Option<String>,
    #[serde(rename = "size", skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    #[serde(rename = "proper")]
    pub proper: bool,
    #[serde(rename = "repack")]
    pub repack: bool,
    #[serde(rename = "retail")]
    pub retail: bool,
    #[serde(rename = "upscaled")]
    pub upscaled: bool,
    #[serde(rename = "remastered")]
    pub remastered: bool,
    #[serde(rename = "unrated")]
    pub unrated: bool,
    #[serde(rename = "documentary")]
    pub documentary: bool,
    #[serde(rename = "episode_code", skip_serializing_if = "Option::is_none")]
    pub episode_code: Option<String>,
    #[serde(rename = "country", skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(rename = "container", skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    #[serde(rename = "extension", skip_serializing_if = "Option::is_none")]
    pub extension: Option<String>,
    #[serde(rename = "torrent")]
    pub torrent: bool,
    #[serde(rename = "category")]
    pub category: String,
    #[serde(rename = "imdb_id", skip_serializing_if = "Option::is_none")]
    pub imdb_id: Option<String>,
    #[serde(rename = "imdb", skip_serializing_if = "Option::is_none")]
    pub imdb: Option<ImdbFile>,
    #[serde(rename = "info_hash")]
    pub info_hash: String,
    #[serde(rename = "adult")]
    pub is_adult: bool,
    #[serde(rename = "ingested_at")]
    pub ingested_at: DateTime<Utc>,
}

impl TorrentInfo {
    /// Build a [`TorrentInfo`] from a [`parsett_rust::ParsedTitle`] plus the
    /// outer context the parser doesn't know (the info hash, the
    /// pre-parse raw title, and the advertised byte size of the torrent).
    ///
    /// This is the single conversion that used to live in two places —
    /// `grpc::mapping::map_torrent_info` (proto path, feeding the DMM
    /// page parser) and the inline struct build in `ingestion::generic`.
    /// Both now call through here, so:
    ///
    /// * there is no proto-hop between parsing and the HTTP/JSON layer,
    /// * every ingestion path shares identical field normalisation, and
    /// * the `category`, `normalized_title`, and `cleaned_parsed_title`
    ///   values are computed exactly once.
    pub fn from_parsed_title(
        info_hash: String,
        raw_title: String,
        bytes: i64,
        parsed: ParsedTitle,
    ) -> Self {
        let normalized = crate::utils::strings::normalize_title(&parsed.title);
        let cleaned = crate::utils::query::clean_query(&parsed.title);
        let category = assign_category(parsed.adult, &parsed.seasons, &parsed.episodes);

        Self {
            info_hash,
            raw_title: Some(raw_title),
            parsed_title: Some(parsed.title),
            normalized_title: Some(normalized),
            cleaned_parsed_title: Some(cleaned),
            trash: parsed.trash,
            year: parsed.year,
            resolution: parsed.resolution,
            seasons: parsed.seasons,
            episodes: parsed.episodes,
            complete: parsed.complete,
            volumes: parsed.volumes,
            // Debug-format the parsett enum variants to the PascalCase
            // strings the `.NET` side used to store (e.g. "English",
            // "BluRay"). This matches the JSON contract consumers are
            // already indexed against.
            languages: parsed
                .languages
                .into_iter()
                .map(|l| format!("{l:?}"))
                .collect(),
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
            size: Some(bytes.to_string()),
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
            imdb_id: None,
            imdb: None,
            is_adult: parsed.adult,
            ingested_at: Utc::now(),
        }
    }

    /// Build a [`TorrentInfo`] from a Postgres row.
    ///
    /// Two call sites feed this function, with slightly different column
    /// sets:
    ///
    /// * `SELECT * FROM "Torrents"` — every column is present, including
    ///   `Trash`, `IsAdult`, `CleanedParsedTitle`. No IMDb aggregation.
    /// * `SELECT ... FROM search_torrents_meta(...)` — returns a subset of
    ///   `Torrents` columns plus the IMDb aliases (`ImdbCategory`,
    ///   `ImdbTitle`, `ImdbYear`, `ImdbAdult`) and a similarity `Score`.
    ///
    /// `try_get_with_default` handles both shapes by treating any missing
    /// column as its type's default, matching the .NET behaviour where
    /// columns unreturned by the stored procedure leave the C# property at
    /// its initialiser value.
    pub fn from_pg_row(row: &PgRow) -> Self {
        let mut t = Self {
            info_hash: get_str(row, "InfoHash").unwrap_or_default(),
            raw_title: get_opt_str(row, "RawTitle"),
            parsed_title: get_opt_str(row, "ParsedTitle"),
            normalized_title: get_opt_str(row, "NormalizedTitle"),
            cleaned_parsed_title: get_opt_str(row, "CleanedParsedTitle"),
            trash: get_bool(row, "Trash"),
            year: get_opt_i32(row, "Year"),
            resolution: get_opt_str(row, "Resolution"),
            seasons: get_vec_i32(row, "Seasons"),
            episodes: get_vec_i32(row, "Episodes"),
            complete: get_bool(row, "Complete"),
            volumes: get_vec_i32(row, "Volumes"),
            languages: get_vec_str(row, "Languages"),
            quality: get_opt_str(row, "Quality"),
            hdr: get_vec_str(row, "Hdr"),
            codec: get_opt_str(row, "Codec"),
            audio: get_vec_str(row, "Audio"),
            channels: get_vec_str(row, "Channels"),
            dubbed: get_bool(row, "Dubbed"),
            subbed: get_bool(row, "Subbed"),
            date: get_opt_str(row, "Date"),
            group: get_opt_str(row, "Group"),
            edition: get_opt_str(row, "Edition"),
            bit_depth: get_opt_str(row, "BitDepth"),
            bitrate: get_opt_str(row, "Bitrate"),
            network: get_opt_str(row, "Network"),
            extended: get_bool(row, "Extended"),
            converted: get_bool(row, "Converted"),
            hardcoded: get_bool(row, "Hardcoded"),
            region: get_opt_str(row, "Region"),
            ppv: get_bool(row, "Ppv"),
            is3d: get_bool(row, "Is3d"),
            site: get_opt_str(row, "Site"),
            size: get_opt_str(row, "Size"),
            proper: get_bool(row, "Proper"),
            repack: get_bool(row, "Repack"),
            retail: get_bool(row, "Retail"),
            upscaled: get_bool(row, "Upscaled"),
            remastered: get_bool(row, "Remastered"),
            unrated: get_bool(row, "Unrated"),
            documentary: get_bool(row, "Documentary"),
            episode_code: get_opt_str(row, "EpisodeCode"),
            country: get_opt_str(row, "Country"),
            container: get_opt_str(row, "Container"),
            extension: get_opt_str(row, "Extension"),
            torrent: get_bool(row, "Torrent"),
            category: get_str(row, "Category").unwrap_or_default(),
            imdb_id: get_opt_str(row, "ImdbId"),
            imdb: None,
            is_adult: get_bool(row, "IsAdult"),
            ingested_at: row.try_get("IngestedAt").unwrap_or_else(|_| Utc::now()),
        };

        // Assemble the nested ImdbFile if the stored procedure returned
        // the joined IMDb columns. For plain SELECTs on "Torrents" these
        // columns are absent, so try_get fails and imdb stays None.
        if let Some(ref imdb_id) = t.imdb_id {
            if let (Ok(category), Ok(title), Ok(year), Ok(adult)) = (
                row.try_get::<Option<String>, _>("ImdbCategory"),
                row.try_get::<Option<String>, _>("ImdbTitle"),
                row.try_get::<Option<i32>, _>("ImdbYear"),
                row.try_get::<Option<bool>, _>("ImdbAdult"),
            ) {
                t.imdb = Some(ImdbFile {
                    imdb_id: imdb_id.clone(),
                    category,
                    title,
                    adult: adult.unwrap_or(false),
                    year: year.unwrap_or(0),
                });
            }
        }

        t
    }
}

// -- column accessors -------------------------------------------------------

fn get_str(row: &PgRow, col: &str) -> Option<String> {
    row.try_get::<String, _>(col).ok()
}

fn get_opt_str(row: &PgRow, col: &str) -> Option<String> {
    row.try_get::<Option<String>, _>(col).unwrap_or(None)
}

fn get_bool(row: &PgRow, col: &str) -> bool {
    row.try_get::<bool, _>(col).unwrap_or(false)
}

fn get_opt_i32(row: &PgRow, col: &str) -> Option<i32> {
    row.try_get::<Option<i32>, _>(col).unwrap_or(None)
}

fn get_vec_i32(row: &PgRow, col: &str) -> Vec<i32> {
    row.try_get::<Vec<i32>, _>(col).unwrap_or_default()
}

fn get_vec_str(row: &PgRow, col: &str) -> Vec<String> {
    row.try_get::<Vec<String>, _>(col).unwrap_or_default()
}

/// Classify a torrent as `movie` / `tvSeries` / `xxx` from parser output.
/// Matches the `assign_category` helper `grpc::mapping` used to hold.
fn assign_category(adult: bool, seasons: &[i32], episodes: &[i32]) -> String {
    if adult {
        "xxx".to_string()
    } else if seasons.is_empty() && episodes.is_empty() {
        "movie".to_string()
    } else {
        "tvSeries".to_string()
    }
}

// -- request / filter types ------------------------------------------------

/// Body of `POST /dmm/search`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DmmQueryRequest {
    pub query_text: String,
}

/// Query string of `GET /dmm/filtered`. Accepts both PascalCase and
/// lowercase parameter names to match .NET's case-insensitive model
/// binding.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SearchFilteredRequest {
    #[serde(default, alias = "Query", alias = "query")]
    pub query: Option<String>,
    #[serde(default, alias = "Season", alias = "season")]
    pub season: Option<i32>,
    #[serde(default, alias = "Episode", alias = "episode")]
    pub episode: Option<i32>,
    #[serde(default, alias = "Year", alias = "year")]
    pub year: Option<i32>,
    #[serde(default, alias = "Language", alias = "language")]
    pub language: Option<String>,
    #[serde(default, alias = "Resolution", alias = "resolution")]
    pub resolution: Option<String>,
    #[serde(default, alias = "ImdbId", alias = "imdbId", alias = "imdbid")]
    pub imdb_id: Option<String>,
    #[serde(default, alias = "Category", alias = "category")]
    pub category: Option<String>,
}

/// Internal filter struct that repositories consume. Mirrors the .NET
/// `TorrentInfoFilter` class. Populated from
/// [`SearchFilteredRequest`] and from the Torznab query translator.
#[derive(Debug, Clone, Default)]
pub struct TorrentInfoFilter {
    pub query: Option<String>,
    pub season: Option<i32>,
    pub episode: Option<i32>,
    pub year: Option<i32>,
    pub language: Option<String>,
    pub resolution: Option<String>,
    pub imdb_id: Option<String>,
    pub category: Option<String>,
}
