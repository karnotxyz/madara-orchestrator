use crate::{config::config, jobs::types::JobStatus};
use async_trait::async_trait;
use std::error::Error;

pub mod data_submission;
pub mod proof_registration;
pub mod proving;
pub mod snos;
pub mod update_state;

#[async_trait]
pub trait Worker: Send + Sync {
    async fn run_worker_if_enabled(&self) -> Result<(), Box<dyn Error>> {
        if !self.is_worker_enabled().await? {
            return Ok(());
        }
        self.run_worker().await
    }

    async fn run_worker(&self) -> Result<(), Box<dyn Error>>;

    // Assumption : False Negative
    // we are assuming that the worker will spawn only 1 job for a block and no two jobs will ever exist
    // for a single block, the code might fail to work as expected if this happens.

    // Checks if any of the jobs have failed
    // Halts any new job creation till all the count of failed jobs is not Zero.
    async fn is_worker_enabled(&self) -> Result<bool, Box<dyn Error>> {
        let config = config().await;

        let failed_jobs = config.database().get_jobs_by_status(JobStatus::VerificationFailed, Some(1)).await?;

        if !failed_jobs.is_empty() {
            return Ok(false);
        }

        Ok(true)
    }
}
