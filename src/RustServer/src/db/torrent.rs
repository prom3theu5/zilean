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

/// Paginated admin listing. Orders newest-first on `IngestedAt`. Accepts
/// an optional `ILIKE` search term against `ParsedTitle`. Unlike the
/// public search, this is a deterministic, operator-facing query — it
/// does not involve pg_trgm similarity scoring.
pub async fn admin_list(
    pool: &PgPool,
    search: Option<&str>,
    per_page: i64,
    offset: i64,
) -> anyhow::Result<(Vec<TorrentInfo>, i64)> {
    let like = search
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| format!("%{s}%"));

    let total: i64 = match like.as_deref() {
        Some(pat) => sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "Torrents" WHERE "ParsedTitle" ILIKE $1"#,
        )
        .bind(pat)
        .fetch_one(pool)
        .await?,
        None => sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Torrents""#)
            .fetch_one(pool)
            .await?,
    };

    let rows = match like.as_deref() {
        Some(pat) => sqlx::query(
            r#"
            SELECT *
            FROM "Torrents"
            WHERE "ParsedTitle" ILIKE $1
            ORDER BY "IngestedAt" DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(pat)
        .bind(per_page)
        .bind(offset)
        .fetch_all(pool)
        .await?,
        None => sqlx::query(
            r#"
            SELECT *
            FROM "Torrents"
            ORDER BY "IngestedAt" DESC
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(per_page)
        .bind(offset)
        .fetch_all(pool)
        .await?,
    };

    Ok((rows.iter().map(TorrentInfo::from_pg_row).collect(), total))
}

/// Return a single torrent count, used by the admin stats card.
pub async fn count(pool: &PgPool) -> anyhow::Result<i64> {
    let n: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Torrents""#)
        .fetch_one(pool)
        .await?;
    Ok(n)
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

/// Look up which of the supplied info hashes are already present in the
/// `Torrents` table. Used by the bulk-insert path to avoid re-sending
/// rows that would hit `ON CONFLICT DO NOTHING` anyway, and by the
/// ingestion pipeline to skip already-known torrents before parsing.
pub async fn existing_hashes(
    pool: &PgPool,
    hashes: &[String],
) -> anyhow::Result<std::collections::HashSet<String>> {
    let mut out = std::collections::HashSet::with_capacity(hashes.len());
    for chunk in hashes.chunks(10_000) {
        let rows: Vec<(String,)> = sqlx::query_as(
            r#"SELECT "InfoHash" FROM "Torrents" WHERE "InfoHash" = ANY($1)"#,
        )
        .bind(chunk)
        .fetch_all(pool)
        .await?;
        out.extend(rows.into_iter().map(|(h,)| h));
    }
    Ok(out)
}

/// Bulk-insert new torrents. Existing rows (same `InfoHash`) are kept
/// unchanged thanks to `ON CONFLICT DO NOTHING`.
///
/// Phase 3 uses a multi-row `INSERT` wrapped in [`sqlx::QueryBuilder`].
/// PostgreSQL caps a single prepared statement at 65 535 parameters, so
/// with 50 columns per row we chunk at 1000 rows per query (50 000 params
/// per batch, safely under the limit).
///
/// This is slower than the .NET `BulkCopyTorrentsAsync` which uses the
/// Npgsql `COPY ... FROM STDIN (FORMAT BINARY)` path; a later phase can
/// drop the multi-row INSERT in favour of an sqlx `copy_in_raw` binary
/// stream once ingestion throughput becomes a bottleneck.
pub async fn bulk_insert(
    pool: &PgPool,
    torrents: &[crate::domain::torrent::TorrentInfo],
) -> anyhow::Result<usize> {
    use sqlx::QueryBuilder;

    let total = torrents.len();
    if total == 0 {
        return Ok(0);
    }

    // 50 columns per row, Postgres limit is 65535 params -> up to 1310
    // rows per statement. 1000 is comfortably under.
    const BATCH: usize = 1000;

    let mut inserted = 0usize;
    for chunk in torrents.chunks(BATCH) {
        let mut qb: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(
            r#"INSERT INTO "Torrents" (
                "InfoHash", "RawTitle", "ParsedTitle", "NormalizedTitle", "CleanedParsedTitle",
                "Trash", "Year", "Resolution", "Seasons", "Episodes", "Complete", "Volumes",
                "Languages", "Quality", "Hdr", "Codec", "Audio", "Channels", "Dubbed", "Subbed",
                "Date", "Group", "Edition", "BitDepth", "Bitrate", "Network", "Extended",
                "Converted", "Hardcoded", "Region", "Ppv", "Is3d", "Site", "Size", "Proper",
                "Repack", "Retail", "Upscaled", "Remastered", "Unrated", "Documentary",
                "EpisodeCode", "Country", "Container", "Extension", "Torrent", "Category",
                "ImdbId", "IsAdult", "IngestedAt"
            ) "#,
        );

        qb.push_values(chunk, |mut b, t| {
            let cleaned = t
                .cleaned_parsed_title
                .clone()
                .or_else(|| {
                    t.parsed_title
                        .as_deref()
                        .map(crate::utils::query::clean_query)
                })
                .unwrap_or_default();
            b.push_bind(&t.info_hash)
                .push_bind(t.raw_title.clone().unwrap_or_default())
                .push_bind(t.parsed_title.clone().unwrap_or_default())
                .push_bind(t.normalized_title.clone().unwrap_or_default())
                .push_bind(cleaned)
                .push_bind(t.trash)
                .push_bind(t.year)
                .push_bind(t.resolution.clone().unwrap_or_default())
                .push_bind(&t.seasons)
                .push_bind(&t.episodes)
                .push_bind(t.complete)
                .push_bind(&t.volumes)
                .push_bind(&t.languages)
                .push_bind(&t.quality)
                .push_bind(&t.hdr)
                .push_bind(&t.codec)
                .push_bind(&t.audio)
                .push_bind(&t.channels)
                .push_bind(t.dubbed)
                .push_bind(t.subbed)
                .push_bind(&t.date)
                .push_bind(&t.group)
                .push_bind(&t.edition)
                .push_bind(&t.bit_depth)
                .push_bind(&t.bitrate)
                .push_bind(&t.network)
                .push_bind(t.extended)
                .push_bind(t.converted)
                .push_bind(t.hardcoded)
                .push_bind(&t.region)
                .push_bind(t.ppv)
                .push_bind(t.is3d)
                .push_bind(&t.site)
                .push_bind(&t.size)
                .push_bind(t.proper)
                .push_bind(t.repack)
                .push_bind(t.retail)
                .push_bind(t.upscaled)
                .push_bind(t.remastered)
                .push_bind(t.unrated)
                .push_bind(t.documentary)
                .push_bind(&t.episode_code)
                .push_bind(&t.country)
                .push_bind(&t.container)
                .push_bind(&t.extension)
                .push_bind(t.torrent)
                .push_bind(&t.category)
                .push_bind(&t.imdb_id)
                .push_bind(t.is_adult)
                .push_bind(t.ingested_at);
        });

        qb.push(r#" ON CONFLICT ("InfoHash") DO NOTHING"#);

        let result = qb.build().execute(pool).await?;
        inserted += result.rows_affected() as usize;
    }

    Ok(inserted)
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
