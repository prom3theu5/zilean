//! Cron scheduling for the background sync jobs.
//!
//! Replaces Coravel's `.Cron(...).PreventOverlapping("SyncJobs")` pattern
//! with [`tokio_cron_scheduler`]. A single named mutex ("SyncJobs") is
//! shared between every job registered through [`Scheduler::schedule`],
//! so two different cron jobs that both claim the mutex will interleave
//! rather than run concurrently — mirroring the .NET behaviour exactly.
//!
//! The Coravel cron strings have five fields (minute, hour, dom, month,
//! dow); `tokio_cron_scheduler` wants six fields (the seconds column in
//! front). [`normalise_cron`] prepends a leading `0 ` to preserve the
//! existing `ZILEAN_DMM_SCRAPE_SCHEDULE` / `ZILEAN_INGESTION_SCRAPE_SCHEDULE`
//! values byte-for-byte.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Context;
use tokio::sync::Mutex;
use tokio_cron_scheduler::{Job, JobScheduler};

/// Wrapper around a `JobScheduler` plus a shared "SyncJobs" mutex.
pub struct Scheduler {
    inner: JobScheduler,
    sync_mutex: Arc<Mutex<()>>,
}

impl Scheduler {
    pub async fn new() -> anyhow::Result<Self> {
        let inner = JobScheduler::new()
            .await
            .context("creating tokio-cron-scheduler")?;
        Ok(Self {
            inner,
            sync_mutex: Arc::new(Mutex::new(())),
        })
    }

    /// Register a job that acquires the shared sync-jobs mutex before
    /// running. If the mutex is held (because another sync job is in
    /// flight) the tick silently no-ops, matching
    /// `PreventOverlapping("SyncJobs")`.
    pub async fn schedule<F>(&self, name: &'static str, cron: &str, f: F) -> anyhow::Result<()>
    where
        F: Fn() -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>
            + Send
            + Sync
            + 'static,
    {
        let cron = normalise_cron(cron);
        let mutex = Arc::clone(&self.sync_mutex);
        let f = Arc::new(f);

        let job = Job::new_async(cron.as_str(), move |_uuid, _l| {
            let mutex = Arc::clone(&mutex);
            let f = Arc::clone(&f);
            Box::pin(async move {
                let Ok(_guard) = mutex.try_lock() else {
                    tracing::debug!(job = name, "SyncJobs mutex busy, skipping tick");
                    return;
                };
                if let Err(err) = f().await {
                    tracing::error!(?err, job = name, "scheduled job failed");
                }
            })
        })
        .with_context(|| format!("registering job `{name}` with cron `{cron}`"))?;

        self.inner.add(job).await?;
        tracing::info!(job = name, cron, "scheduled");
        Ok(())
    }

    /// Begin running scheduled jobs.
    pub async fn start(&self) -> anyhow::Result<()> {
        self.inner.start().await?;
        Ok(())
    }

    /// Acquire the shared mutex for a one-shot on-demand run (used by
    /// `/dmm/on-demand-scrape` to avoid overlapping a concurrent cron
    /// tick).
    pub fn sync_mutex(&self) -> Arc<Mutex<()>> {
        Arc::clone(&self.sync_mutex)
    }
}

/// Convert a five-field Coravel-style cron expression to the six-field
/// form tokio-cron-scheduler expects (prepends `0 ` for the seconds
/// column). Six- or seven-field expressions pass through unchanged so
/// operators can opt in to sub-minute precision if they want it.
pub fn normalise_cron(expr: &str) -> String {
    let trimmed = expr.trim();
    let field_count = trimmed.split_whitespace().count();
    match field_count {
        5 => format!("0 {trimmed}"),
        _ => trimmed.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_field_gets_seconds_prefix() {
        assert_eq!(normalise_cron("0 * * * *"), "0 0 * * * *");
        assert_eq!(normalise_cron("*/15 * * * *"), "0 */15 * * * *");
    }

    #[test]
    fn six_or_more_field_passes_through() {
        assert_eq!(normalise_cron("*/30 0 * * * *"), "*/30 0 * * * *");
        assert_eq!(normalise_cron("0 0 * * * * 2026"), "0 0 * * * * 2026");
    }

    #[test]
    fn trims_whitespace() {
        assert_eq!(normalise_cron("  0 * * * *  "), "0 0 * * * *");
    }
}
