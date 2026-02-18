pub mod reminders;
pub mod tasks;

use anyhow::{Context, Result};
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::info;

/// Wrapper around tokio-cron-scheduler for background tasks
pub struct Scheduler {
    inner: JobScheduler,
}

impl Scheduler {
    /// Create a new scheduler
    pub async fn new() -> Result<Self> {
        let inner = JobScheduler::new()
            .await
            .context("Failed to create job scheduler")?;
        Ok(Self { inner })
    }

    /// Add a recurring cron job
    pub async fn add_cron_job<F>(&self, cron_expr: &str, name: &str, task: F) -> Result<()>
    where
        F: Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
            + Send
            + Sync
            + 'static,
    {
        let job_name = name.to_string();
        let job = Job::new_async(cron_expr, move |_uuid, _lock| {
            let name = job_name.clone();
            let fut = task();
            Box::pin(async move {
                info!("Running scheduled task: {}", name);
                fut.await;
            })
        })
        .with_context(|| format!("Failed to create cron job: {}", name))?;

        self.inner
            .add(job)
            .await
            .with_context(|| format!("Failed to add job: {}", name))?;

        info!("Scheduled task '{}' with cron: {}", name, cron_expr);
        Ok(())
    }

    /// Start the scheduler
    pub async fn start(&self) -> Result<()> {
        self.inner
            .start()
            .await
            .context("Failed to start scheduler")?;
        info!("Scheduler started");
        Ok(())
    }

    /// Shutdown the scheduler
    #[allow(dead_code)]
    pub async fn shutdown(&mut self) -> Result<()> {
        self.inner
            .shutdown()
            .await
            .context("Failed to shutdown scheduler")?;
        info!("Scheduler stopped");
        Ok(())
    }
}
