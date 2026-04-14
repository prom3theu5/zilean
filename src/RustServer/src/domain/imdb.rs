//! IMDb-facing types.

use serde::{Deserialize, Serialize};

/// IMDb entry nested inside a [`TorrentInfo`] JSON payload. Matches the
/// .NET `ImdbFile` class which serialises with ASP.NET Core's default
/// camelCase policy (no explicit `[JsonPropertyName]` attributes).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImdbFile {
    pub imdb_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub adult: bool,
    pub year: i32,
}

/// Single result from `POST /imdb/search`. Fields mirror the .NET
/// `ImdbSearchResult` class, which inherits the default camelCase JSON
/// policy.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImdbSearchResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imdb_id: Option<String>,
    pub year: i32,
    pub score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}

/// Query-string-bound request for `POST /imdb/search`. Property names match
/// the .NET `ImdbFilteredRequest` class (PascalCase on the wire because
/// ASP.NET's query binding is case-insensitive; callers historically send
/// either casing, so we accept both).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ImdbFilteredRequest {
    #[serde(default, alias = "Query", alias = "query")]
    pub query: Option<String>,
    #[serde(default, alias = "Year", alias = "year")]
    pub year: Option<i32>,
    #[serde(default, alias = "Category", alias = "category")]
    pub category: Option<String>,
}
