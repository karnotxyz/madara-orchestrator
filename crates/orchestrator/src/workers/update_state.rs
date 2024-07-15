use std::error::Error;

use crate::config::config;
use crate::jobs::create_job;
use crate::jobs::types::JobType;
use async_trait::async_trait;

use crate::workers::Worker;

pub struct UpdateStateWorker;

#[async_trait]
impl Worker for UpdateStateWorker {
    /// 1. Fetch the last successful state update job
    /// 2. Fetch all successful proving jobs covering blocks after the last state update
    /// 3. Create state updates for all the blocks that don't have a state update job
    async fn run_worker(&self) -> Result<(), Box<dyn Error>> {
        let config = config().await;
        let latest_successful_job = config.database().get_last_successful_job_by_type(JobType::StateTransition).await?;

        match latest_successful_job {
            Some(job) => {
                let latest_successful_job_internal_id = job.internal_id;

                let successful_proving_jobs = config
                    .database()
                    .get_completed_jobs_after_internal_id_by_job_type(
                        JobType::ProofCreation,
                        latest_successful_job_internal_id,
                    )
                    .await?;

                for job in successful_proving_jobs {
                    let existing_job = config
                        .database()
                        .get_job_by_internal_id_and_type(&job.internal_id, &JobType::StateTransition)
                        .await?;
                    match existing_job {
                        Some(job) => {
                            log::info!("State Update Job already exists for internal id : {}", job.internal_id)
                        }
                        None => {
                            create_job(JobType::StateTransition, job.internal_id, job.metadata).await?;
                        }
                    }
                }

                Ok(())
            }
            None => {
                log::info!("No successful state update jobs found");
                return Ok(());
            }
        }
    }
}
