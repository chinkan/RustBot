use tracing::info;

use crate::memory::MemoryStore;
use crate::scheduler::Scheduler;

/// Register built-in background tasks
pub async fn register_builtin_tasks(
    scheduler: &Scheduler,
    _memory: MemoryStore,
) -> anyhow::Result<()> {
    // Heartbeat — log that the bot is alive every hour
    scheduler
        .add_cron_job("0 0 * * * *", "heartbeat", || {
            Box::pin(async {
                info!("Heartbeat: bot is alive");
            })
        })
        .await?;

    Ok(())
}
