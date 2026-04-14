//! Response shapes specific to the `/torrents/*` endpoints.

use serde::{Deserialize, Serialize};

use super::torrent::TorrentInfo;

/// One entry in the JSON array streamed by `GET /torrents/all`.
///
/// Field names match the .NET `StreamedEntry` class exactly:
/// `name` (not `title`), `size` (int64, in bytes), and `hash`
/// (not `info_hash`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamedEntry {
    #[serde(rename = "name")]
    pub name: String,
    #[serde(rename = "size")]
    pub size: i64,
    #[serde(rename = "hash")]
    pub info_hash: String,
}

/// One entry in the response from `GET /torrents/checkcached`.
#[derive(Debug, Clone, Serialize)]
pub struct CachedItem {
    #[serde(rename = "info_hash")]
    pub info_hash: String,
    #[serde(rename = "is_cached")]
    pub is_cached: bool,
    #[serde(rename = "item", skip_serializing_if = "Option::is_none")]
    pub item: Option<TorrentInfo>,
}

/// Query-string-bound request for `GET /torrents/checkcached`. The `.NET`
/// version accepts `?hashes=...` as a single comma-separated string.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CheckCachedRequest {
    #[serde(default, alias = "Hashes", alias = "hashes")]
    pub hashes: Option<String>,
}

/// Query-string-bound request for `DELETE /blacklist/remove`. Kept here
/// rather than in `domain::blacklist` because the parameter name
/// (`infoHash`, PascalCase-camelCase mix) differs from the body used by
/// `PUT /blacklist/add`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct BlacklistRemoveRequest {
    #[serde(default, alias = "InfoHash", alias = "infoHash", alias = "info_hash")]
    pub info_hash: Option<String>,
}

/// Canonical JSON error body, matching .NET's `ErrorResponse { message }`.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorResponse {
    #[serde(rename = "message")]
    pub message: String,
}

impl ErrorResponse {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}
