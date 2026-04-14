// src/main.rs
mod configuration;
mod db;
mod dmm;
mod grpc;
mod imdb;
mod utils;

use crate::configuration::config::load_config;

pub mod proto {
    tonic::include_proto!("zilean_rust");
}

use grpc::server::start_server;
use tracing_subscriber::EnvFilter;

#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let app_config = load_config()?;
    start_server(app_config).await
}
