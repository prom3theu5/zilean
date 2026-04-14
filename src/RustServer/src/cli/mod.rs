//! Command-line interface.
//!
//! Replaces the combination of .NET `Zilean.ApiService` entry point and
//! the `Zilean.Scraper` executable with a single binary whose behaviour
//! is driven by the chosen subcommand:
//!
//! * `serve` (or no subcommand at all) — runs the HTTP and gRPC servers
//!   and the scheduler. Preserves the historical no-arg invocation the
//!   .NET app used to launch the Rust binary, so existing deployments
//!   don't need reconfiguring mid-migration.
//! * `dmm-sync` — runs the DMM ingestion once, then exits. Replaces
//!   `scraper dmm-sync`.
//! * `generic-sync` — runs the Zurg/Zilean/Generic ingestion once, then
//!   exits. Replaces `scraper generic-sync`.
//! * `resync-imdb` — rebuilds the IMDb index and optionally retags
//!   torrents. Replaces `scraper resync-imdb` with the same flag
//!   semantics (`-d -i -t -a`).

use clap::{Parser, Subcommand};

/// Command-line entry point for the Zilean binary.
#[derive(Debug, Parser)]
#[command(
    name = "zilean",
    version,
    about = "Zilean: DebridMediaManager indexer and search service.",
    long_about = "Zilean scrapes and indexes DMM hash lists, surfaces them via a Torznab\n\
                  indexer, and maintains an IMDb title index for matching. Invoked without a\n\
                  subcommand, the binary boots the full server (HTTP + gRPC + scheduler).\n\
                  Subcommands trigger one-shot maintenance operations and exit."
)]
pub struct Cli {
    /// Optional subcommand. When omitted the binary runs in `serve` mode,
    /// matching the historical no-arg invocation used by the .NET wrapper.
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the HTTP + gRPC servers and the scheduler.
    Serve,

    /// Synchronise the DMM hash repo into the Torrents table, then exit.
    #[command(name = "dmm-sync", alias = "dmm")]
    DmmSync,

    /// Pull configured Zurg/Zilean/Generic endpoints and ingest their
    /// torrents, then exit.
    #[command(name = "generic-sync", alias = "generic")]
    GenericSync,

    /// Rebuild the IMDb index and optionally retag existing torrents.
    #[command(name = "resync-imdb", alias = "imdb")]
    ResyncImdb {
        /// Skip the 30-day cache check and force a fresh IMDb download.
        #[arg(short = 'd', long = "force-download")]
        force_download: bool,

        /// Rebuild the Tantivy index even when a valid one already exists.
        #[arg(short = 'i', long = "force-create-index")]
        force_create_index: bool,

        /// Match IMDb ids for torrents that currently have none.
        #[arg(short = 't', long = "retag-missing-imdbs", conflicts_with = "retag_all_imdbs")]
        retag_missing_imdbs: bool,

        /// Match IMDb ids for every non-adult torrent.
        #[arg(short = 'a', long = "retag-all-imdbs")]
        retag_all_imdbs: bool,
    },
}

impl Cli {
    /// Resolve the effective command — defaulting to [`Command::Serve`]
    /// when none was supplied.
    pub fn resolved_command(self) -> Command {
        self.command.unwrap_or(Command::Serve)
    }
}
