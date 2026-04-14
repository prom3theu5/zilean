//! Shared domain types used by the HTTP layer, the ingestion pipeline, and
//! the database repositories.
//!
//! These structs are the Rust equivalents of the models under
//! `src/Zilean.Shared/Features/` on the .NET side. JSON field names and
//! nullability are matched exactly so existing HTTP clients (Sonarr,
//! Radarr, Zurg, other Zilean instances) do not need reconfiguring.

#![allow(dead_code)] // Phase 2: types are produced here, consumed over the
// course of Phases 2-4.

pub mod blacklist;
pub mod imdb;
pub mod torrent;
pub mod torrents_api;
