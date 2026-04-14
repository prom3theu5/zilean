//! Database access layer.
//!
//! This module is the single place where Zilean opens the Postgres
//! connection pool and applies its schema. The schema is defined by the SQL
//! files under `src/RustServer/migrations/` and applied by [`migrate::run`].
//!
//! During Phase 1 of the .NET → Rust migration we only own the schema and
//! the connection pool. Repository types (Torrents, Blacklist, IMDb, etc.)
//! are introduced in later phases as the HTTP/CLI callers that need them
//! are ported.
//!
//! The `dead_code` allow covers the Phase 1 reality that these helpers are
//! defined but not yet wired into the existing gRPC startup path; Phase 2
//! removes the allow.

#![allow(dead_code)]

pub mod blacklist;
pub mod imdb_file;
pub mod migrate;
pub mod pool;
pub mod torrent;
