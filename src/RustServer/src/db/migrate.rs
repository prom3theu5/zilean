//! Schema migrations.
//!
//! `sqlx::migrate!` embeds the contents of `src/RustServer/migrations/` into
//! the binary at compile time. [`run`] applies any migration that has not
//! already been recorded in the `_sqlx_migrations` tracking table.
//!
//! The baseline migration is idempotent, so calling [`run`] on a database
//! that was previously managed by the .NET Entity Framework Core migrations
//! is safe and effectively a no-op. The `__EFMigrationsHistory` table that
//! EF leaves behind is never touched and can be dropped manually once the
//! .NET app has been retired.

use sqlx::PgPool;
use sqlx::migrate::Migrator;

/// The embedded migrator. Exposed as `pub(crate)` so tests or other bootstrap
/// code can inspect it (for example, to list versions or compute checksums).
pub(crate) static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// Apply all pending migrations to `pool`.
///
/// Callers should invoke this once at startup, before accepting traffic or
/// spawning any background jobs that touch the database.
pub async fn run(pool: &PgPool) -> anyhow::Result<()> {
    tracing::info!("Applying Zilean schema migrations...");
    MIGRATOR.run(pool).await?;
    tracing::info!("Schema migrations up to date.");
    Ok(())
}
