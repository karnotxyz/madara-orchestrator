use rstest::*;

use std::collections::HashMap;

use httpmock::prelude::*;


use super::super::common::{default_job_item, init_config};
use uuid::Uuid;

use crate::jobs::types::ExternalId;
use crate::jobs::{
    register_proof_job::RegisterProofJob,
    types::{JobItem, JobStatus, JobType},
    Job,
};
use settlement_client_interface::{SettlementVerificationStatus, MockSettlementClient};

#[rstest]
#[tokio::test]
async fn test_create_job() {
    // TODO: Update this url to a valid proof url
    let proof_url = "https://jsonplaceholder.typicode.com/todos/1".to_string();
    let config = init_config(None, None, None, None, None).await;

    let mut hash_map = HashMap::new();
    hash_map.insert("proof_url".to_string(), proof_url.to_string());

    let job = RegisterProofJob.create_job(&config, String::from("0"), hash_map).await;
    assert!(job.is_ok());

    let job = job.unwrap();

    let job_type = job.job_type;
    assert_eq!(job_type, JobType::ProofRegistration, "job_type should be ProofRegistration");
    assert!(!(job.id.is_nil()), "id should not be nil");
    assert_eq!(job.status, JobStatus::Created, "status should be Created");
    assert_eq!(job.version, 0_i32, "version should be 0");
    assert_eq!(job.external_id.unwrap_string().unwrap(), String::new(), "external_id should be empty string");
    assert_eq!(job.metadata.get("proof_url").unwrap().to_string(), proof_url, "proof_url should be valid");
}

#[rstest]
#[tokio::test]
async fn test_verify_job(#[from(default_job_item)] job_item: JobItem) {
    let mut settlement_client = MockSettlementClient::new();
    settlement_client.expect_verify_inclusion().times(1).returning(|_| Ok(SettlementVerificationStatus::Verified));

    let config = init_config(None, None, None, None, Some(settlement_client)).await;
    assert!(RegisterProofJob.verify_job(&config, &job_item).await.is_ok());
}

#[rstest]
#[tokio::test]
async fn test_process_job() {
    let server = MockServer::start();

    let mut hash_map = HashMap::new();
    // TODO: Update this url to a valid proof url
    let proof_url = "https://jsonplaceholder.typicode.com/todos/1".to_string();
    hash_map.insert("proof_url".to_string(), proof_url.to_string());

    let mut settlement_client = MockSettlementClient::new();
    let internal_id = "1";
    settlement_client.expect_register_proof().times(1).returning(|_| Ok(internal_id.to_string()));
    let config = init_config(Some(format!("http://localhost:{}", server.port())), None, None, None, Some(settlement_client)).await;
    assert_eq!(
        RegisterProofJob
            .process_job(
                &config,
                &JobItem {
                    id: Uuid::default(),
                    internal_id: internal_id.to_string(),
                    job_type: JobType::ProofRegistration,
                    status: JobStatus::Created,
                    external_id: ExternalId::String("1".to_string().into_boxed_str()),
                    metadata: hash_map,
                    version: 0,
                }
            )
            .await
            .unwrap(),
        internal_id
    );

}
