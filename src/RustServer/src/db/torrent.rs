//! Repository over the `Torrents` table and the `search_torrents_meta`
//! stored procedure.
//!
//! The Phase-2 HTTP handlers read through here. Ingestion-time writers
//! (bulk COPY) are added in Phase 3 when the in-process scraper lands.

use futures::Stream;
use sqlx::{PgPool, Row, postgres::PgRow};

use crate::domain::torrent::{TorrentInfo, TorrentInfoFilter};
use crate::domain::torrents_api::StreamedEntry;

/// Equivalent of `ITorrentInfoService.SearchForTorrentInfoByOnlyTitle` on
/// the .NET side. Runs a pg_trgm similarity match against `ParsedTitle`
/// and returns up to 100 rows sorted by Postgres's operator (no explicit
/// ORDER BY, matching the original query).
pub async fn search_by_title(pool: &PgPool, query: &str) -> anyhow::Result<Vec<TorrentInfo>> {
    // The input goes through `Parsing.CleanQuery()` on the .NET side; we
    // mirror that in util::query::clean so the similarity match behaves the
    // same. The caller hands us the already-cleaned query.
    let rows = sqlx::query(
        r#"
        SELECT *
        FROM "Torrents"
        WHERE "ParsedTitle" % $1
          AND length("InfoHash") = 40
        LIMIT 100
        "#,
    )
    .bind(query)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(TorrentInfo::from_pg_row).collect())
}

/// Equivalent of `ITorrentInfoService.SearchForTorrentInfoFiltered`.
/// Forwards to the `search_torrents_meta` stored procedure and folds the
/// IMDb join columns into the nested `TorrentInfo.imdb` field.
pub async fn search_filtered(
    pool: &PgPool,
    filter: &TorrentInfoFilter,
    limit: i32,
    similarity_threshold: f32,
) -> anyhow::Result<Vec<TorrentInfo>> {
    let rows = sqlx::query(
        r#"
        SELECT *
        FROM search_torrents_meta(
            $1,  -- query
            $2,  -- season
            $3,  -- episode
            $4,  -- year
            $5,  -- language
            $6,  -- resolution
            $7,  -- imdbId
            $8,  -- limit_param
            $9,  -- category
            $10  -- similarity_threshold
        )
        "#,
    )
    .bind(&filter.query)
    .bind(filter.season)
    .bind(filter.episode)
    .bind(filter.year)
    .bind(&filter.language)
    .bind(&filter.resolution)
    .bind(ensure_imdb_prefix(filter.imdb_id.as_deref()))
    .bind(limit)
    .bind(&filter.category)
    .bind(similarity_threshold)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(TorrentInfo::from_pg_row).collect())
}

/// Lookup the rows matching an arbitrary list of info hashes. Used by
/// `GET /torrents/checkcached` to decide which hashes are present.
pub async fn find_by_hashes(
    pool: &PgPool,
    hashes: &[String],
) -> anyhow::Result<Vec<TorrentInfo>> {
    let rows = sqlx::query(
        r#"
        SELECT *
        FROM "Torrents"
        WHERE "InfoHash" = ANY($1)
        "#,
    )
    .bind(hashes)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(TorrentInfo::from_pg_row).collect())
}

/// Produce a [`Stream`] of [`StreamedEntry`] for `GET /torrents/all`.
///
/// The .NET implementation walks the EF Core `IAsyncEnumerable`. We drop
/// down to sqlx's `fetch` which streams rows directly off the wire.
pub fn stream_all<'a>(
    pool: &'a PgPool,
) -> impl Stream<Item = Result<StreamedEntry, sqlx::Error>> + 'a {
    use futures::StreamExt;
    sqlx::query(r#"SELECT "InfoHash", "RawTitle", "Size" FROM "Torrents""#)
        .fetch(pool)
        .map(|row| row.map(streamed_entry_from_row))
}

fn streamed_entry_from_row(row: PgRow) -> StreamedEntry {
    let info_hash: String = row.try_get("InfoHash").unwrap_or_default();
    let name: String = row.try_get("RawTitle").unwrap_or_default();
    // `Size` is stored as text on the .NET side because the upstream feed is
    // a human-readable string. Empty or non-numeric sizes become 0, which
    // matches `long.Parse` rejecting the value — .NET actually throws there,
    // but we prefer to skip over malformed rows silently on a streaming
    // endpoint rather than abort the whole response.
    let size_raw: String = row.try_get("Size").unwrap_or_default();
    let size = size_raw.parse::<i64>().unwrap_or(0);
    StreamedEntry {
        name,
        size,
        info_hash,
    }
}

/// Delete a single torrent by info hash. Returns true if a row was removed.
/// Used by the blacklist flow, which removes a torrent from `Torrents` as a
/// side effect of blacklisting its hash.
pub async fn delete_by_hash(pool: &PgPool, info_hash: &str) -> anyhow::Result<bool> {
    let result = sqlx::query(r#"DELETE FROM "Torrents" WHERE "InfoHash" = $1"#)
        .bind(info_hash)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// VACUUM (VERBOSE, ANALYZE) "Torrents". Ingestion code calls this after
/// a bulk COPY to reclaim space and update planner statistics.
pub async fn vacuum_analyze(pool: &PgPool) -> anyhow::Result<()> {
    // VACUUM cannot run inside a transaction, so we acquire a dedicated
    // connection and execute in autocommit mode.
    let mut conn = pool.acquire().await?;
    sqlx::query(r#"VACUUM (VERBOSE, ANALYZE) "Torrents""#)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Prefix a bare IMDb id with `tt` the way `TorrentInfoService.EnsureCorrectFormatImdbId`
/// does on the .NET side, so `search_torrents_meta` never sees a dangling id.
fn ensure_imdb_prefix(id: Option<&str>) -> Option<String> {
    id.map(|id| {
        if id.is_empty() {
            String::new()
        } else if id.starts_with("tt") {
            id.to_owned()
        } else {
            format!("tt{id}")
        }
    })
    .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_imdb_prefix_passthrough() {
        assert_eq!(ensure_imdb_prefix(Some("tt1234567")).as_deref(), Some("tt1234567"));
    }

    #[test]
    fn ensure_imdb_prefix_adds_tt() {
        assert_eq!(ensure_imdb_prefix(Some("1234567")).as_deref(), Some("tt1234567"));
    }

    #[test]
    fn ensure_imdb_prefix_none() {
        assert!(ensure_imdb_prefix(None).is_none());
    }

    #[test]
    fn ensure_imdb_prefix_empty() {
        assert!(ensure_imdb_prefix(Some("")).is_none());
    }
}
