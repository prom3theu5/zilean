//! In-process ingestion pipelines.
//!
//! Replaces the .NET `Zilean.Scraper` executable with Rust implementations
//! that share the same process as the HTTP server and the scheduler.
//! Submodules:
//!
//! * [`dmm`]      — walks the DMM hash repo (git) and stores parsed
//!                  torrents in the `Torrents` table.
//! * [`generic`]  — streams torrents from Zurg/Zilean/Generic HTTP
//!                  endpoints.
//! * [`imdb`]     — rebuilds the IMDb Tantivy index and (optionally)
//!                  back-fills IMDb ids on existing torrents.
//! * [`kubernetes`] — feature-gated Kubernetes service discovery.

pub mod dmm;
pub mod generic;
pub mod imdb;
#[cfg(feature = "kubernetes")]
pub mod kubernetes;
