//! Torznab server-side implementation.
//!
//! The contract is defined at <https://torznab.github.io/>; Sonarr and
//! Radarr are the canonical clients Zilean is tuned for. This module
//! mirrors the .NET `Zilean.Shared.Features.Torznab` namespace piece by
//! piece:
//!
//! * [`categories`]: Torznab category tree (Movies, TV, XXX, etc.) and
//!   category-id → internal-category-label mapping.
//! * [`caps`]: XML writer for `?t=caps`.
//! * [`result_page`]: XML writer for search responses (RSS 2.0 with the
//!   `torznab` and `atom` namespaces).
//! * [`error`]: XML writer for error responses.
//! * [`query`]: parser that converts the raw query string into a
//!   [`query::TorznabQuery`] and then to a
//!   [`crate::domain::torrent::TorrentInfoFilter`].
//! * [`guid`]: deterministic RFC-4122-style GUID derived from an info
//!   hash; matches the C# `Parsing.CreateGuidFromInfohash` byte layout so
//!   existing Sonarr/Radarr indexer caches don't churn.
//! * [`xml_utils`]: RFC-822 date formatter and invalid-XML character
//!   stripper shared by the writers above.

pub mod caps;
pub mod categories;
pub mod error;
pub mod guid;
pub mod query;
pub mod result_page;
pub mod xml_utils;
