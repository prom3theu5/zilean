//! Configuration loading.
//!
//! All configuration is read from the environment with the prefix `ZILEAN_`.
//! The struct is deliberately flat: a configuration crate that tries to
//! auto-nest on `_` would make the existing env-var names (`ZILEAN_DMM_LOCAL_PATH`,
//! etc.) ambiguous with new nested sections. Flat fields keep behaviour
//! explicit and documentation easy.

use config::Config;
use rayon::ThreadPoolBuilder;
use tracing::info;

#[derive(Debug, Clone, Default, serde::Deserialize, PartialEq)]
pub(crate) struct AppConfig {
    // --- Phase 0/1: existing settings that have always been read from env --

    /// Thread count for the Rayon pool used during torrent-title parsing and
    /// DMM page processing. Minimum 1.
    pub parsing_threads: usize,

    /// libpq-style Postgres connection string. Required.
    pub database_url: String,

    /// Upstream git repo that hosts the DMM hash list. Cloned/pulled by the
    /// DMM ingestion pipeline.
    pub dmm_repo_url: String,

    /// Local filesystem path where the DMM repo is cached.
    pub dmm_local_path: String,

    /// Minimum fuzzy-match score (0.0–1.0) used by the Tantivy-backed IMDb
    /// searcher to filter out weak matches.
    pub imdb_minimum_score: f32,

    // --- Phase 2: HTTP listener -------------------------------------------

    /// TCP port the HTTP server listens on. `None` (the default, env var
    /// unset) means the HTTP listener is not started — this preserves the
    /// pre-Phase 2 behaviour where the Rust binary only opened the gRPC
    /// Unix socket.
    #[serde(default)]
    pub http_port: Option<u16>,

    /// Interface the HTTP listener binds to. Only honoured when
    /// `http_port` is set.
    pub http_bind: String,

    /// API key used by endpoints that require authentication. When `None`
    /// those endpoints return 401 Unauthorized for every request (the same
    /// behaviour the .NET app exhibited when `Zilean:ApiKey` was blank).
    #[serde(default)]
    pub api_key: Option<String>,

    // --- Phase 2: endpoint toggles (match the .NET EnableEndpoint flags) --

    pub dmm_enabled: bool,
    pub torznab_enabled: bool,
    pub imdb_enabled: bool,
    pub torrents_scrape_enabled: bool,
    pub torrents_cache_check_enabled: bool,

    // --- Phase 2: limits and thresholds -----------------------------------

    /// Maximum rows returned by `GET /dmm/filtered`. Default 200, matching
    /// the .NET `Dmm.MaxFilteredResults`.
    pub dmm_max_filtered_results: i32,

    /// Similarity threshold (0.0-1.0) forwarded to `search_torrents_meta`
    /// and `search_imdb_meta`. Default 0.85, matching `Dmm.MinimumScoreMatch`
    /// and `Imdb.MinimumScoreMatch` on the .NET side.
    pub dmm_minimum_score: f32,

    /// Maximum hashes accepted by `GET /torrents/checkcached`. Default 100.
    pub torrents_max_hashes_to_check: usize,

    // --- Phase 3: scheduler + ingestion ----------------------------------

    /// Whether DMM scraping runs on the in-process scheduler when the
    /// binary is in `serve` mode. Matches `Dmm.EnableScraping`.
    pub dmm_scraping_enabled: bool,

    /// Cron expression for the DMM sync. Accepts either a five-field or
    /// six-field string; `scheduler::normalise_cron` prepends the seconds
    /// column if absent.
    pub dmm_scrape_schedule: String,

    /// Whether generic ingestion (Zurg / Zilean / Generic endpoints) runs
    /// on the scheduler. Matches `Ingestion.EnableScraping`.
    pub ingestion_scraping_enabled: bool,

    /// Cron expression for the generic ingestion.
    pub ingestion_scrape_schedule: String,

    /// Static endpoints for generic ingestion, represented as a
    /// JSON-encoded array so the flat config layout can stay env-var-
    /// friendly. Example:
    ///   [{"kind":"Zurg","url":"http://zurg:9999"}, ...]
    #[serde(default)]
    pub ingestion_endpoints: Option<String>,
}

pub fn load_config() -> anyhow::Result<AppConfig> {
    let config = Config::builder()
        // Phase 0/1 defaults (unchanged).
        .set_default("parsing_threads", 4)?
        .set_default(
            "dmm_repo_url",
            "https://github.com/debridmediamanager/hashlists.git",
        )?
        .set_default("dmm_local_path", "./data/dmm-hashlists")?
        .set_default("imdb_minimum_score", 0.85)?
        // Phase 2 defaults.
        .set_default("http_bind", "0.0.0.0")?
        .set_default("dmm_enabled", true)?
        .set_default("torznab_enabled", true)?
        .set_default("imdb_enabled", true)?
        .set_default("torrents_scrape_enabled", false)?
        .set_default("torrents_cache_check_enabled", false)?
        .set_default("dmm_max_filtered_results", 200)?
        .set_default("dmm_minimum_score", 0.85)?
        .set_default("torrents_max_hashes_to_check", 100)?
        // Phase 3 defaults (match .NET EnableScraping / ScrapeSchedule).
        .set_default("dmm_scraping_enabled", true)?
        .set_default("dmm_scrape_schedule", "0 * * * *")?
        .set_default("ingestion_scraping_enabled", false)?
        .set_default("ingestion_scrape_schedule", "0 * * * *")?
        .add_source(config::Environment::with_prefix("ZILEAN"))
        .build()?;

    let app_config: AppConfig = config.try_deserialize()?;

    if app_config.parsing_threads == 0 {
        return Err(anyhow::anyhow!(
            "ZILEAN_PARSING_THREADS must be greater than 0"
        ));
    }

    if app_config.database_url.is_empty() {
        return Err(anyhow::anyhow!("ZILEAN_DATABASE_URL must be set"));
    }

    if app_config.imdb_minimum_score < 0.0 || app_config.imdb_minimum_score > 1.0 {
        return Err(anyhow::anyhow!(
            "ZILEAN_IMDB_MINIMUM_SCORE must be between 0.0 and 1.0"
        ));
    }

    if app_config.dmm_minimum_score < 0.0 || app_config.dmm_minimum_score > 1.0 {
        return Err(anyhow::anyhow!(
            "ZILEAN_DMM_MINIMUM_SCORE must be between 0.0 and 1.0"
        ));
    }

    info!("Using {} parsing threads.", &app_config.parsing_threads);
    info!(
        "Using IMDB minimum score: {}",
        &app_config.imdb_minimum_score
    );
    info!("Using DMM repo URL: {}", &app_config.dmm_repo_url);
    info!("Using DMM local path: {}", &app_config.dmm_local_path);
    match app_config.http_port {
        Some(port) => info!(
            "HTTP listener will bind to {}:{}",
            &app_config.http_bind, port
        ),
        None => info!("HTTP listener disabled (set ZILEAN_HTTP_PORT to enable)"),
    }

    ThreadPoolBuilder::new()
        .num_threads(app_config.parsing_threads)
        .build_global()
        .expect("Rayon pool already initialized");

    Ok(app_config)
}
