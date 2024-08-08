use std::collections::HashMap;

use ::mongodb::bson::doc;
use async_trait::async_trait;
use color_eyre::Result;
use mockall::automock;
use uuid::Uuid;

use crate::jobs::types::{JobItem, JobStatus, JobType};

/// MongoDB
pub mod mongodb;

/// The Database trait is used to define the methods that a database
/// should implement to be used as a storage for the orchestrator. The
/// purpose of this trait is to allow developers to use any DB of their choice
/// as long as they implement the trait
///
/// The Database should support optimistic locking. For example, assume we've two threads
/// A and B and both read the same Job entry J at nearly the same time. If A updates J at
/// time T1 and then B updates J at time T2 (T2>T1), then B's update should fail because
/// it's version of J is outdated.
#[automock]
#[async_trait]
pub trait Database: Send + Sync {
    async fn create_job(&self, job: JobItem) -> Result<JobItem>;
    async fn get_job_by_id(&self, id: Uuid) -> Result<Option<JobItem>>;
    async fn get_job_by_internal_id_and_type(&self, internal_id: &str, job_type: &JobType) -> Result<Option<JobItem>>;
    async fn update_job(&self, job: &JobItem) -> Result<()>;
    async fn update_job_status(&self, job: &JobItem, new_status: JobStatus) -> Result<()>;
    async fn update_metadata(&self, job: &JobItem, metadata: HashMap<String, String>) -> Result<()>;
    async fn get_latest_job_by_type(&self, job_type: JobType) -> Result<Option<JobItem>>;
    async fn get_jobs_without_successor(
        &self,
        job_a_type: JobType,
        job_a_status: JobStatus,
        job_b_type: JobType,
    ) -> Result<Vec<JobItem>>;
    async fn get_latest_job_by_type_and_status(
        &self,
        job_type: JobType,
        job_status: JobStatus,
    ) -> Result<Option<JobItem>>;
    async fn get_jobs_after_internal_id_by_job_type(
        &self,
        job_type: JobType,
        internal_id: String,
    ) -> Result<Vec<JobItem>>;

    // TODO: can be extendible to support multiple status.
    async fn get_jobs_by_statuses(&self, status: Vec<JobStatus>, limit: Option<i64>) -> Result<Vec<JobItem>>;
}

pub trait DatabaseConfig {
    fn new_from_env() -> Self;
}
