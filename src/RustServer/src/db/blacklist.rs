//! Repository over the `BlacklistedItems` table.

use chrono::Utc;
use sqlx::PgPool;

/// Check whether a torrent is already blacklisted.
pub async fn exists(pool: &PgPool, info_hash: &str) -> anyhow::Result<bool> {
    let row = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(1) FROM "BlacklistedItems" WHERE "InfoHash" = $1"#,
    )
    .bind(info_hash)
    .fetch_one(pool)
    .await?;
    Ok(row > 0)
}

/// Insert a new blacklist row. The caller is responsible for checking
/// [`exists`] first if it wants to distinguish "created" from "already
/// there" (the .NET handler returns 409 Conflict in the latter case).
pub async fn insert(
    pool: &PgPool,
    info_hash: &str,
    reason: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO "BlacklistedItems" ("InfoHash", "Reason", "BlacklistedAt")
        VALUES ($1, $2, $3)
        "#,
    )
    .bind(info_hash)
    .bind(reason)
    .bind(Utc::now())
    .execute(pool)
    .await?;
    Ok(())
}

/// Remove a blacklist row. Returns true if a row was actually deleted, so
/// the handler can return 404 vs 204 correctly.
pub async fn remove(pool: &PgPool, info_hash: &str) -> anyhow::Result<bool> {
    let result = sqlx::query(r#"DELETE FROM "BlacklistedItems" WHERE "InfoHash" = $1"#)
        .bind(info_hash)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}
