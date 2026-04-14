//! Postgres connection pool construction.
//!
//! All repository code takes a borrowed [`sqlx::PgPool`]; this is the one
//! place where we actually build it. A small pool is deliberate: Zilean has
//! a handful of concurrent call sites (HTTP handlers, scheduled jobs,
//! streaming ingestion) and every additional connection is a resource the
//! Postgres server must reserve. Callers that need bulk COPY should acquire
//! a dedicated connection via [`sqlx::PgPool::acquire`] rather than widen
//! the pool.

use std::time::Duration;

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

/// Default maximum pool size. Matches the value `PgDmmDbService` has used
/// since the Rust gRPC server was introduced, so behaviour is unchanged for
/// existing deployments.
pub const DEFAULT_MAX_CONNECTIONS: u32 = 5;

/// Build a Postgres pool from a connection string.
///
/// The `database_url` is the usual libpq-style URL (`postgres://user:pw@host/db`)
/// or a key/value string that Postgres's client libraries accept. sqlx parses
/// both forms.
pub async fn connect(database_url: &str) -> anyhow::Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(DEFAULT_MAX_CONNECTIONS)
        // Acquire timeout shields the process from hanging forever if the
        // DB is temporarily unreachable during startup. 30s is long enough
        // for a slow container boot but short enough that an operator
        // notices a misconfigured connection string quickly.
        .acquire_timeout(Duration::from_secs(30))
        .connect(database_url)
        .await?;

    Ok(pool)
}
