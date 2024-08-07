use super::database::build_job_item;
use crate::config::config;
use crate::jobs::handle_job_failure;
use rstest::rstest;
use std::str::FromStr;
#[cfg(test)]
pub mod da_job;

#[cfg(test)]
pub mod proving_job;

#[cfg(test)]
pub mod state_update_job;

use assert_matches::assert_matches;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use mockall::predicate::eq;
use mongodb::bson::doc;
use omniqueue::QueueError;
use tokio::time::sleep;
use uuid::Uuid;

use crate::jobs::constants::{JOB_PROCESS_ATTEMPT_METADATA_KEY, JOB_VERIFICATION_ATTEMPT_METADATA_KEY};
use crate::jobs::job_handler_factory::mock_factory;
use crate::jobs::types::{ExternalId, JobItem, JobStatus, JobType, JobVerificationStatus};
use crate::jobs::{create_job, increment_key_in_metadata, process_job, verify_job, Job, MockJob};
use crate::queue::job_queue::{JOB_PROCESSING_QUEUE, JOB_VERIFICATION_QUEUE};
use crate::tests::common::MessagePayloadType;
use crate::tests::config::TestConfigBuilder;

/// Tests `create_job` function when job is not existing in the db.
#[rstest]
#[tokio::test]
async fn create_job_job_does_not_exists_in_db_works() {
    let job_item = build_job_item_by_type_and_status(JobType::SnosRun, JobStatus::Created, "0".to_string());
    let mut job_handler = MockJob::new();

    // Adding expectation for creation of new job.
    let job_item_clone = job_item.clone();
    job_handler.expect_create_job().times(1).returning(move |_, _, _| Ok(job_item_clone.clone()));

    TestConfigBuilder::new().build().await;
    let config = config().await;

    // Mocking the `get_job_handler` call in create_job function.
    let job_handler: Arc<Box<dyn Job>> = Arc::new(Box::new(job_handler));
    let ctx = mock_factory::get_job_handler_context();
    ctx.expect().times(1).with(eq(JobType::SnosRun)).return_once(move |_| Arc::clone(&job_handler));

    assert!(create_job(JobType::SnosRun, "0".to_string(), HashMap::new()).await.is_ok());

    let mut hashmap: HashMap<String, String> = HashMap::new();
    hashmap.insert(JOB_PROCESS_ATTEMPT_METADATA_KEY.to_string(), "0".to_string());
    hashmap.insert(JOB_VERIFICATION_ATTEMPT_METADATA_KEY.to_string(), "0".to_string());

    // Db checks.
    let job_in_db = config.database().get_job_by_id(job_item.id).await.unwrap().unwrap();
    assert_eq!(job_in_db.id, job_item.id);
    assert_eq!(job_in_db.internal_id, job_item.internal_id);
    assert_eq!(job_in_db.metadata, hashmap);

    // Waiting for 5 secs for message to be passed into the queue
    sleep(Duration::from_secs(5)).await;

    // Queue checks.
    let consumed_messages = config.queue().consume_message_from_queue(JOB_PROCESSING_QUEUE.to_string()).await.unwrap();
    let consumed_message_payload: MessagePayloadType = consumed_messages.payload_serde_json().unwrap().unwrap();
    assert_eq!(consumed_message_payload.id, job_item.id);
}

/// Tests `create_job` function when job is already existing in the db.
#[rstest]
#[tokio::test]
async fn create_job_job_exists_in_db_works() {
    let job_item = build_job_item_by_type_and_status(JobType::ProofCreation, JobStatus::Created, "0".to_string());

    TestConfigBuilder::new().build().await;

    let config = config().await;
    let database_client = config.database();
    database_client.create_job(job_item).await.unwrap();

    assert!(create_job(JobType::ProofCreation, "0".to_string(), HashMap::new()).await.is_err());

    // Waiting for 5 secs for message to be passed into the queue
    sleep(Duration::from_secs(5)).await;

    // Queue checks.
    let consumed_messages =
        config.queue().consume_message_from_queue(JOB_PROCESSING_QUEUE.to_string()).await.unwrap_err();
    assert_matches!(consumed_messages, QueueError::NoData);
}

/// Tests `create_job` function when job handler is not implemented in the `get_job_handler`
/// This test should fail as job handler is not implemented in the `factory.rs`
#[rstest]
#[should_panic(expected = "Job type not implemented yet.")]
#[tokio::test]
async fn create_job_job_handler_is_not_implemented_panics() {
    TestConfigBuilder::new().build().await;
    let config = config().await;

    // Mocking the `get_job_handler` call in create_job function.
    let ctx = mock_factory::get_job_handler_context();
    ctx.expect().times(1).returning(|_| panic!("Job type not implemented yet."));

    assert!(create_job(JobType::ProofCreation, "0".to_string(), HashMap::new()).await.is_err());

    // Waiting for 5 secs for message to be passed into the queue
    sleep(Duration::from_secs(5)).await;

    // Queue checks.
    let consumed_messages =
        config.queue().consume_message_from_queue(JOB_PROCESSING_QUEUE.to_string()).await.unwrap_err();
    assert_matches!(consumed_messages, QueueError::NoData);
}

/// Tests `process_job` function when job is already existing in the db and job status is either
/// `Created` or `VerificationFailed`.
#[rstest]
#[case(JobType::SnosRun, JobStatus::Created)]
#[case(JobType::DataSubmission, JobStatus::VerificationFailed)]
#[tokio::test]
async fn process_job_with_job_exists_in_db_and_valid_job_processing_status_works(
    #[case] job_type: JobType,
    #[case] job_status: JobStatus,
) {
    let job_item = build_job_item_by_type_and_status(job_type.clone(), job_status.clone(), "1".to_string());

    // Building config
    TestConfigBuilder::new().build().await;
    let config = config().await;
    let database_client = config.database();

    let mut job_handler = MockJob::new();

    // Creating job in database
    database_client.create_job(job_item.clone()).await.unwrap();
    // Expecting process job function in job processor to return the external ID.
    job_handler.expect_process_job().times(1).returning(move |_, _| Ok("0xbeef".to_string()));
    job_handler.expect_verification_polling_delay_seconds().return_const(1u64);

    // Mocking the `get_job_handler` call in create_job function.
    let job_handler: Arc<Box<dyn Job>> = Arc::new(Box::new(job_handler));
    let ctx = mock_factory::get_job_handler_context();
    ctx.expect().times(1).with(eq(job_type.clone())).returning(move |_| Arc::clone(&job_handler));

    assert!(process_job(job_item.id).await.is_ok());
    // Getting the updated job.
    let updated_job = database_client.get_job_by_id(job_item.id).await.unwrap().unwrap();
    // checking if job_status is updated in db
    assert_eq!(updated_job.status, JobStatus::PendingVerification);
    assert_eq!(updated_job.external_id, ExternalId::String(Box::from("0xbeef")));
    assert_eq!(updated_job.metadata.get(JOB_PROCESS_ATTEMPT_METADATA_KEY).unwrap(), "1");

    // Waiting for 5 secs for message to be passed into the queue
    sleep(Duration::from_secs(5)).await;

    // Queue checks
    let consumed_messages =
        config.queue().consume_message_from_queue(JOB_VERIFICATION_QUEUE.to_string()).await.unwrap();
    let consumed_message_payload: MessagePayloadType = consumed_messages.payload_serde_json().unwrap().unwrap();
    assert_eq!(consumed_message_payload.id, job_item.id);
}

/// Tests `process_job` function when job is already existing in the db and job status is not
/// `Created` or `VerificationFailed`.
#[rstest]
#[tokio::test]
async fn process_job_with_job_exists_in_db_with_invalid_job_processing_status_errors() {
    // Creating a job with Completed status which is invalid processing.
    let job_item = build_job_item_by_type_and_status(JobType::SnosRun, JobStatus::Completed, "1".to_string());

    // building config
    TestConfigBuilder::new().build().await;
    let config = config().await;
    let database_client = config.database();

    // creating job in database
    database_client.create_job(job_item.clone()).await.unwrap();

    assert!(process_job(job_item.id).await.is_err());

    let job_in_db = database_client.get_job_by_id(job_item.id).await.unwrap().unwrap();
    // Job should be untouched in db.
    assert_eq!(job_in_db, job_item);

    // Waiting for 5 secs for message to be passed into the queue
    sleep(Duration::from_secs(5)).await;

    // Queue checks.
    let consumed_messages =
        config.queue().consume_message_from_queue(JOB_VERIFICATION_QUEUE.to_string()).await.unwrap_err();
    assert_matches!(consumed_messages, QueueError::NoData);
}

/// Tests `process_job` function when job is not in the db
/// This test should fail
#[rstest]
#[tokio::test]
async fn process_job_job_does_not_exists_in_db_works() {
    // Creating a valid job which is not existing in the db.
    let job_item = build_job_item_by_type_and_status(JobType::SnosRun, JobStatus::Created, "1".to_string());

    // building config
    TestConfigBuilder::new().build().await;
    let config = config().await;

    assert!(process_job(job_item.id).await.is_err());

    // Waiting for 5 secs for message to be passed into the queue
    sleep(Duration::from_secs(5)).await;

    // Queue checks.
    let consumed_messages =
        config.queue().consume_message_from_queue(JOB_VERIFICATION_QUEUE.to_string()).await.unwrap_err();
    assert_matches!(consumed_messages, QueueError::NoData);
}

/// Tests `process_job` function when 2 workers try to process the same job.
/// This test should fail because once the job is locked for processing on one
/// worker it should not be accessed by another worker and should throw an error
/// when updating the job status.
#[rstest]
#[tokio::test]
async fn process_job_two_workers_process_same_job_works() {
    let mut job_handler = MockJob::new();
    // Expecting process job function in job processor to return the external ID.
    job_handler.expect_process_job().times(1).returning(move |_, _| Ok("0xbeef".to_string()));
    job_handler.expect_verification_polling_delay_seconds().return_const(1u64);

    // Mocking the `get_job_handler` call in create_job function.
    let job_handler: Arc<Box<dyn Job>> = Arc::new(Box::new(job_handler));
    let ctx = mock_factory::get_job_handler_context();
    ctx.expect().times(1).with(eq(JobType::SnosRun)).returning(move |_| Arc::clone(&job_handler));

    // building config
    TestConfigBuilder::new().build().await;
    let config = config().await;
    let db_client = config.database();

    let job_item = build_job_item_by_type_and_status(JobType::SnosRun, JobStatus::Created, "1".to_string());

    // Creating the job in the db
    db_client.create_job(job_item.clone()).await.unwrap();

    // Simulating the two workers
    let worker_1 = tokio::spawn(async move { process_job(job_item.id).await });
    let worker_2 = tokio::spawn(async move { process_job(job_item.id).await });

    // waiting for workers to complete the processing
    let (result_1, result_2) = tokio::join!(worker_1, worker_2);

    assert_ne!(
        result_1.unwrap().is_ok(),
        result_2.unwrap().is_ok(),
        "One worker should succeed and the other should fail"
    );

    // Waiting for 5 secs for job to be updated in the db
    sleep(Duration::from_secs(5)).await;

    let final_job_in_db = db_client.get_job_by_id(job_item.id).await.unwrap().unwrap();
    assert_eq!(final_job_in_db.status, JobStatus::PendingVerification);
}

/// Tests `verify_job` function when job is having expected status
/// and returns a `Verified` verification status.
#[rstest]
#[tokio::test]
async fn verify_job_with_verified_status_works() {
    let job_item =
        build_job_item_by_type_and_status(JobType::DataSubmission, JobStatus::PendingVerification, "1".to_string());

    // building config
    TestConfigBuilder::new().build().await;

    let config = config().await;
    let database_client = config.database();
    let mut job_handler = MockJob::new();

    // creating job in database
    database_client.create_job(job_item.clone()).await.unwrap();
    // expecting process job function in job processor to return the external ID
    job_handler.expect_verify_job().times(1).returning(move |_, _| Ok(JobVerificationStatus::Verified));
    job_handler.expect_max_process_attempts().returning(move || 2u64);

    let job_handler: Arc<Box<dyn Job>> = Arc::new(Box::new(job_handler));
    let ctx = mock_factory::get_job_handler_context();
    // Mocking the `get_job_handler` call in create_job function.
    ctx.expect().times(1).with(eq(JobType::DataSubmission)).returning(move |_| Arc::clone(&job_handler));

    assert!(verify_job(job_item.id).await.is_ok());

    // DB checks.
    let updated_job = database_client.get_job_by_id(job_item.id).await.unwrap().unwrap();
    assert_eq!(updated_job.status, JobStatus::Completed);

    // Waiting for 5 secs for message to be passed into the queue
    sleep(Duration::from_secs(5)).await;

    // Queue checks.
    let consumed_messages_verification_queue =
        config.queue().consume_message_from_queue(JOB_VERIFICATION_QUEUE.to_string()).await.unwrap_err();
    assert_matches!(consumed_messages_verification_queue, QueueError::NoData);
    let consumed_messages_processing_queue =
        config.queue().consume_message_from_queue(JOB_PROCESSING_QUEUE.to_string()).await.unwrap_err();
    assert_matches!(consumed_messages_processing_queue, QueueError::NoData);
}

/// Tests `verify_job` function when job is having expected status
/// and returns a `Rejected` verification status.
#[rstest]
#[tokio::test]
async fn verify_job_with_rejected_status_adds_to_queue_works() {
    let job_item =
        build_job_item_by_type_and_status(JobType::DataSubmission, JobStatus::PendingVerification, "1".to_string());

    // building config
    TestConfigBuilder::new().build().await;

    let config = config().await;
    let database_client = config.database();
    let mut job_handler = MockJob::new();

    // creating job in database
    database_client.create_job(job_item.clone()).await.unwrap();
    job_handler.expect_verify_job().times(1).returning(move |_, _| Ok(JobVerificationStatus::Rejected("".to_string())));
    job_handler.expect_max_process_attempts().returning(move || 2u64);

    let job_handler: Arc<Box<dyn Job>> = Arc::new(Box::new(job_handler));
    let ctx = mock_factory::get_job_handler_context();
    // Mocking the `get_job_handler` call in create_job function.
    ctx.expect().times(1).with(eq(JobType::DataSubmission)).returning(move |_| Arc::clone(&job_handler));

    assert!(verify_job(job_item.id).await.is_ok());

    // DB checks.
    let updated_job = database_client.get_job_by_id(job_item.id).await.unwrap().unwrap();
    assert_eq!(updated_job.status, JobStatus::VerificationFailed);

    // Waiting for 5 secs for message to be passed into the queue
    sleep(Duration::from_secs(5)).await;

    // Queue checks.
    let consumed_messages = config.queue().consume_message_from_queue(JOB_PROCESSING_QUEUE.to_string()).await.unwrap();
    let consumed_message_payload: MessagePayloadType = consumed_messages.payload_serde_json().unwrap().unwrap();
    assert_eq!(consumed_message_payload.id, job_item.id);
}

/// Tests `verify_job` function when job is having expected status
/// and returns a `Rejected` verification status but doesn't add
/// the job to process queue because of maximum attempts reached.
#[rstest]
#[tokio::test]
async fn verify_job_with_rejected_status_works() {
    let mut job_item =
        build_job_item_by_type_and_status(JobType::DataSubmission, JobStatus::PendingVerification, "1".to_string());

    // increasing JOB_VERIFICATION_ATTEMPT_METADATA_KEY to simulate max. attempts reached.
    let metadata = increment_key_in_metadata(&job_item.metadata, JOB_PROCESS_ATTEMPT_METADATA_KEY).unwrap();
    job_item.metadata = metadata;

    // building config
    TestConfigBuilder::new().build().await;

    let config = config().await;
    let database_client = config.database();
    let mut job_handler = MockJob::new();

    // creating job in database
    database_client.create_job(job_item.clone()).await.unwrap();
    // expecting process job function in job processor to return the external ID
    job_handler.expect_verify_job().times(1).returning(move |_, _| Ok(JobVerificationStatus::Rejected("".to_string())));
    job_handler.expect_max_process_attempts().returning(move || 1u64);

    let job_handler: Arc<Box<dyn Job>> = Arc::new(Box::new(job_handler));
    let ctx = mock_factory::get_job_handler_context();
    // Mocking the `get_job_handler` call in create_job function.
    ctx.expect().times(1).with(eq(JobType::DataSubmission)).returning(move |_| Arc::clone(&job_handler));

    assert!(verify_job(job_item.id).await.is_ok());

    // DB checks.
    let updated_job = database_client.get_job_by_id(job_item.id).await.unwrap().unwrap();
    assert_eq!(updated_job.status, JobStatus::VerificationFailed);
    assert_eq!(updated_job.metadata.get(JOB_PROCESS_ATTEMPT_METADATA_KEY).unwrap(), "1");

    // Waiting for 5 secs for message to be passed into the queue
    sleep(Duration::from_secs(5)).await;

    // Queue checks.
    let consumed_messages_processing_queue =
        config.queue().consume_message_from_queue(JOB_PROCESSING_QUEUE.to_string()).await.unwrap_err();
    assert_matches!(consumed_messages_processing_queue, QueueError::NoData);
}

/// Tests `verify_job` function when job is having expected status
/// and returns a `Pending` verification status.
#[rstest]
#[tokio::test]
async fn verify_job_with_pending_status_adds_to_queue_works() {
    let job_item =
        build_job_item_by_type_and_status(JobType::DataSubmission, JobStatus::PendingVerification, "1".to_string());

    // building config
    TestConfigBuilder::new().build().await;

    let config = config().await;
    let database_client = config.database();
    let mut job_handler = MockJob::new();

    // creating job in database
    database_client.create_job(job_item.clone()).await.unwrap();
    // expecting process job function in job processor to return the external ID
    job_handler.expect_verify_job().times(1).returning(move |_, _| Ok(JobVerificationStatus::Pending));
    job_handler.expect_max_verification_attempts().returning(move || 2u64);
    job_handler.expect_verification_polling_delay_seconds().returning(move || 2u64);

    let job_handler: Arc<Box<dyn Job>> = Arc::new(Box::new(job_handler));
    let ctx = mock_factory::get_job_handler_context();
    // Mocking the `get_job_handler` call in create_job function.
    ctx.expect().times(1).with(eq(JobType::DataSubmission)).returning(move |_| Arc::clone(&job_handler));

    assert!(verify_job(job_item.id).await.is_ok());

    // DB checks.
    let updated_job = database_client.get_job_by_id(job_item.id).await.unwrap().unwrap();
    assert_eq!(updated_job.metadata.get(JOB_VERIFICATION_ATTEMPT_METADATA_KEY).unwrap(), "1");
    assert_eq!(updated_job.status, JobStatus::PendingVerification);

    // Waiting for 5 secs for message to be passed into the queue
    sleep(Duration::from_secs(5)).await;

    // Queue checks
    let consumed_messages =
        config.queue().consume_message_from_queue(JOB_VERIFICATION_QUEUE.to_string()).await.unwrap();
    let consumed_message_payload: MessagePayloadType = consumed_messages.payload_serde_json().unwrap().unwrap();
    assert_eq!(consumed_message_payload.id, job_item.id);
}

/// Tests `verify_job` function when job is having expected status
/// and returns a `Pending` verification status but doesn't add
/// the job to process queue because of maximum attempts reached.
#[rstest]
#[tokio::test]
async fn verify_job_with_pending_status_works() {
    let mut job_item =
        build_job_item_by_type_and_status(JobType::DataSubmission, JobStatus::PendingVerification, "1".to_string());

    // increasing JOB_VERIFICATION_ATTEMPT_METADATA_KEY to simulate max. attempts reached.
    let metadata = increment_key_in_metadata(&job_item.metadata, JOB_VERIFICATION_ATTEMPT_METADATA_KEY).unwrap();
    job_item.metadata = metadata;

    // building config
    TestConfigBuilder::new().build().await;

    let config = config().await;
    let database_client = config.database();
    let mut job_handler = MockJob::new();

    // creating job in database
    database_client.create_job(job_item.clone()).await.unwrap();
    // expecting process job function in job processor to return the external ID
    job_handler.expect_verify_job().times(1).returning(move |_, _| Ok(JobVerificationStatus::Pending));
    job_handler.expect_max_verification_attempts().returning(move || 1u64);
    job_handler.expect_verification_polling_delay_seconds().returning(move || 2u64);

    let job_handler: Arc<Box<dyn Job>> = Arc::new(Box::new(job_handler));
    let ctx = mock_factory::get_job_handler_context();
    // Mocking the `get_job_handler` call in create_job function.
    ctx.expect().times(1).with(eq(JobType::DataSubmission)).returning(move |_| Arc::clone(&job_handler));

    assert!(verify_job(job_item.id).await.is_ok());

    // DB checks.
    let updated_job = database_client.get_job_by_id(job_item.id).await.unwrap().unwrap();
    assert_eq!(updated_job.status, JobStatus::VerificationTimeout);
    assert_eq!(updated_job.metadata.get(JOB_VERIFICATION_ATTEMPT_METADATA_KEY).unwrap(), "1");

    // Waiting for 5 secs for message to be passed into the queue
    sleep(Duration::from_secs(5)).await;

    // Queue checks.
    let consumed_messages_verification_queue =
        config.queue().consume_message_from_queue(JOB_VERIFICATION_QUEUE.to_string()).await.unwrap_err();
    assert_matches!(consumed_messages_verification_queue, QueueError::NoData);
}

fn build_job_item_by_type_and_status(job_type: JobType, job_status: JobStatus, internal_id: String) -> JobItem {
    let mut hashmap: HashMap<String, String> = HashMap::new();
    hashmap.insert(JOB_PROCESS_ATTEMPT_METADATA_KEY.to_string(), "0".to_string());
    hashmap.insert(JOB_VERIFICATION_ATTEMPT_METADATA_KEY.to_string(), "0".to_string());
    JobItem {
        id: Uuid::new_v4(),
        internal_id,
        job_type,
        status: job_status,
        external_id: ExternalId::Number(0),
        metadata: hashmap,
        version: 0,
    }
}

#[cfg(test)]
impl FromStr for JobStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Created" => Ok(JobStatus::Created),
            "LockedForProcessing" => Ok(JobStatus::LockedForProcessing),
            "PendingVerification" => Ok(JobStatus::PendingVerification),
            "Completed" => Ok(JobStatus::Completed),
            "VerificationTimeout" => Ok(JobStatus::VerificationTimeout),
            "VerificationFailed" => Ok(JobStatus::VerificationFailed),
            "Failed" => Ok(JobStatus::Failed),
            s if s.starts_with("VerificationFailed(") && s.ends_with(')') => {
                let reason = s[19..s.len() - 1].to_string();
                Ok(JobStatus::VerificationFailed)
            }
            _ => Err(format!("Invalid job status: {}", s)),
        }
    }
}

#[cfg(test)]
impl FromStr for JobType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "SnosRun" => Ok(JobType::SnosRun),
            "DataSubmission" => Ok(JobType::DataSubmission),
            "ProofCreation" => Ok(JobType::ProofCreation),
            "ProofRegistration" => Ok(JobType::ProofRegistration),
            "StateTransition" => Ok(JobType::StateTransition),
            _ => Err(format!("Invalid job type: {}", s)),
        }
    }
}

#[rstest]
#[case("SnosRun", "PendingVerification")]
#[case("DataSubmission", "Failed")]
#[tokio::test]
async fn handle_job_failure_job_status_typical_works(#[case] job_type: JobType, #[case] job_status: JobStatus) {
    TestConfigBuilder::new().build().await;
    let internal_id = 1;

    let config = config().await;
    let database_client = config.database();

    // create a job
    let mut job = build_job_item(job_type.clone(), job_status.clone(), internal_id);
    let job_id = job.id;

    // if test case is for Failure, add last_job_status to job's metadata
    if job_status == JobStatus::Failed {
        let mut metadata = job.metadata.clone();
        metadata.insert("last_job_status".to_string(), "VerificationTimeout".to_string());
        job.metadata = metadata;
    }

    // feeding the job to DB
    database_client.create_job(job.clone()).await.unwrap();

    // calling handle_job_failure
    let response = handle_job_failure(job_id).await;

    match response {
        Ok(()) => {
            // check job in db
            let job = config.database().get_job_by_id(job_id).await.expect("Unable to fetch Job Data");

            if let Some(job_item) = job {
                // check if job status is Failure
                assert_eq!(job_item.status, JobStatus::Failed);
                // check if job metadata has `last_job_status`
                assert_ne!(None, job_item.metadata.get("last_job_status"));

                println!("Handle Job Failure for ID {} was handled successfully", job_id);
            }
        }
        Err(err) => {
            panic!("Test case should have passed: {} ", err);
        }
    }
}

#[rstest]
// code should panic here, how can completed move to dl queue ?
#[case("DataSubmission", "Completed")]
#[tokio::test]
async fn handle_job_failure__job_status_completed_fails(#[case] job_type: JobType, #[case] job_status: JobStatus) {
    use color_eyre::eyre::eyre;

    TestConfigBuilder::new().build().await;
    let internal_id = 1;

    let config = config().await;
    let database_client = config.database();

    // create a job
    let job = build_job_item(job_type.clone(), job_status.clone(), internal_id);
    let job_id = job.id;

    // feeding the job to DB
    database_client.create_job(job.clone()).await.unwrap();

    // calling handle_job_failure
    let response = handle_job_failure(job_id).await;

    match response {
        Ok(()) => {
            panic!("Test call to handle_job_failure should not have passed");
        }
        Err(err) => {
            // Should only fail for Completed case, anything else : raise error
            let expected = eyre!("Invalid state exists on DL queue: {}", job_status);
            assert_eq!(err.to_string(), expected.to_string());
        }
    }
}
