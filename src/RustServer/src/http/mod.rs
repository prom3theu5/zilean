//! HTTP surface for the Rust server.
//!
//! The module owns the axum router, the per-request [`state::AppState`],
//! API-key authentication middleware, and the handler implementations for
//! every endpoint the .NET API currently exposes. The shape of each
//! handler deliberately mirrors the .NET `EndpointMapper` extension
//! methods under `src/Zilean.ApiService/Features/`, so operators can reach
//! for the same URL and request shape they're already using.

pub mod auth;
pub mod routes;
pub mod server;
pub mod state;

pub use server::serve;
