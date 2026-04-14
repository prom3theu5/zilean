//! Shared HTTP application state.
//!
//! Every handler receives an `AppState` via axum's `State` extractor.
//! The state is cheap to clone (it's an `Arc`-of-`Arc`s under the hood).
//!
//! Phase 4 grew the state to carry the Tantivy searcher plus the shared
//! sync-jobs mutex so the `/dmm/on-demand-scrape` handler can dispatch a
//! DMM sync without overlapping the scheduler's tick.

use std::sync::Arc;

use arc_swap::ArcSwap;
use sqlx::PgPool;
use tokio::sync::Mutex;

use crate::configuration::config::AppConfig;
use crate::imdb::ImdbSearcher;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<AppConfig>,
    pub searcher: Arc<ArcSwap<ImdbSearcher>>,
    pub sync_mutex: Arc<Mutex<()>>,
}

impl AppState {
    pub fn new(
        db: PgPool,
        config: Arc<AppConfig>,
        searcher: Arc<ArcSwap<ImdbSearcher>>,
        sync_mutex: Arc<Mutex<()>>,
    ) -> Self {
        Self {
            db,
            config,
            searcher,
            sync_mutex,
        }
    }
}
