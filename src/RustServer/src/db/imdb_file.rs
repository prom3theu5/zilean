//! Repository over the `ImdbFiles` table and the `search_imdb_meta` stored
//! procedure.

use sqlx::{PgPool, Row};

use crate::domain::imdb::ImdbSearchResult;

/// Equivalent of `IImdbFileService.SearchForImdbIdAsync`. Forwards to the
/// `search_imdb_meta` stored procedure and fans the rows out into
/// [`ImdbSearchResult`]. Returns at most `limit` rows (the stored procedure
/// enforces this server-side).
pub async fn search(
    pool: &PgPool,
    query: &str,
    year: Option<i32>,
    category: Option<&str>,
    limit: i32,
    similarity_threshold: f32,
) -> anyhow::Result<Vec<ImdbSearchResult>> {
    let rows = sqlx::query(
        r#"
        SELECT imdb_id, title, category, year, score
        FROM search_imdb_meta($1, $2, $3, $4, $5)
        "#,
    )
    .bind(query)
    .bind(category)
    .bind(year)
    .bind(limit)
    .bind(similarity_threshold)
    .fetch_all(pool)
    .await?;

    let results = rows
        .into_iter()
        .map(|row| ImdbSearchResult {
            imdb_id: row.try_get("imdb_id").ok(),
            title: row.try_get("title").ok().flatten(),
            category: row.try_get("category").ok().flatten(),
            year: row.try_get("year").unwrap_or(0),
            // `score` comes back as a `real` (f32); the .NET DTO widens to
            // double on the response. Do the same to keep the JSON numeric
            // representation identical.
            score: row.try_get::<f32, _>("score").map(|s| s as f64).unwrap_or(0.0),
        })
        .collect();

    Ok(results)
}
