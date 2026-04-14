//! Blacklist-facing types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Persisted row for the `BlacklistedItems` table. The .NET version has
/// explicit `[JsonPropertyName]` attributes producing snake_case JSON field
/// names, which is what we mirror here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct BlacklistedItem {
    #[serde(rename = "info_hash")]
    #[sqlx(rename = "InfoHash")]
    pub info_hash: String,
    #[serde(rename = "reason")]
    #[sqlx(rename = "Reason")]
    pub reason: String,
    #[serde(rename = "blacklisted_at")]
    #[sqlx(rename = "BlacklistedAt")]
    pub blacklisted_at: DateTime<Utc>,
}

/// Query-string-bound request body for `PUT /blacklist/add`. The .NET
/// counterpart uses lowercase-with-underscores property names
/// (`info_hash`, `reason`) by virtue of lower-cased C# property names, not
/// via JSON attributes, so clients send exactly those wire names.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct BlacklistItemRequest {
    #[serde(default)]
    pub info_hash: String,
    #[serde(default)]
    pub reason: String,
}
