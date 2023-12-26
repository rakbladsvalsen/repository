use std::time::Duration;

use central_repository_config::inner::Config;
use log::{error, info, warn};
use tokio::time::{interval, timeout};

use crate::UploadSessionMutation;

pub struct Tasks;

impl Tasks {
    pub fn init_prune_task() {
        if Config::get().enable_prune_job {
            tokio::spawn(Self::prune_periodically());
        } else {
            warn!("Prune job is disabled. This will cause increased storage usage.");
        }
    }

    async fn prune_periodically() {
        let config = Config::get();
        let mut sleep = interval(Duration::from_secs(config.prune_job_run_interval_seconds));
        let duration = Duration::from_secs(config.prune_job_timeout_seconds);
        info!(
            "Pruner task: sleep duration: {}s, timeout: {}s.",
            config.prune_job_run_interval_seconds, config.prune_job_timeout_seconds
        );
        loop {
            sleep.tick().await;
            let prune_fn = timeout(duration, UploadSessionMutation::prune_old_items());
            match prune_fn.await {
                Ok(Ok(prune_result)) => info!(
                    "pruner task: successfully pruned {} formats",
                    prune_result.len()
                ),
                Ok(Err(e)) => error!("pruner task: error during pruning: {:#?}", e),
                Err(e) => error!("pruner task: timeout during pruning: {:#?}", e),
            };
        }
    }
}
