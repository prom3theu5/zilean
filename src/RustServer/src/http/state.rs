//! Shared HTTP application state.
//!
//! All handlers receive an `AppState` via axum's `State` extractor. The
//! state is cheap to clone (it's an `Arc` over a struct of `Arc`s
//! internally).

use std::sync::Arc;

use sqlx::PgPool;

use crate::configuration::config::AppConfig;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<AppConfig>,
}

impl AppState {
    pub fn new(db: PgPool, config: Arc<AppConfig>) -> Self {
        Self { db, config }
    }
}
