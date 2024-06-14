use crate::config::Config;
use crate::jobs::types::{JobItem, JobStatus, JobType, JobVerificationStatus};
use crate::jobs::Job;
use async_trait::async_trait;
use color_eyre::Result;
use std::collections::HashMap;
use uuid::Uuid;
use starknet::core::types::FieldElement;


pub struct RegisterProofJob;

#[async_trait]
impl Job for RegisterProofJob {
    async fn create_job(
        &self,
        _config: &Config,
        internal_id: String,
        metadata: HashMap<String, String>,
    ) -> Result<JobItem> {
        Ok(JobItem {
            id: Uuid::new_v4(),
            internal_id,
            job_type: JobType::ProofRegistration,
            status: JobStatus::Created,
            external_id: String::new().into(),
            // metadata must contain the blocks that have been included inside this proof
            // this will allow state update jobs to be created for each block
            metadata,
            version: 0,
        })
    }

    async fn process_job(&self, _config: &Config, _job: &JobItem) -> Result<String> {
        // Get proof from S3 and submit on chain for verification
        // We need to implement a generic trait for this to support multiple
        // base layers
        let proof_url = _job.metadata.get("proof_url").unwrap().to_string();

        let proof_data = reqwest::get(&proof_url).await?.text().await?;
        
        let chunks = proof_data.trim_start_matches("0x").as_bytes().chunks(64);

        let mut proof_data_vec: Vec<FieldElement> = vec![];
        chunks.for_each(|chunk| {
            let s = std::str::from_utf8(chunk).expect("Invalid UTF-8");
            proof_data_vec.push(FieldElement::from_dec_str(s).expect("Invalid FieldElement"));
        });

        let external_id = _config.settlement_client().register_proof(proof_data_vec).await?;

        Ok(external_id)
    }

    async fn verify_job(&self, _config: &Config, _job: &JobItem) -> Result<JobVerificationStatus> {
        Ok(_config.settlement_client().verify_inclusion(_job.external_id.unwrap_string()?).await?.into())
    }

    fn max_process_attempts(&self) -> u64 {
        1
    }

    fn max_verification_attempts(&self) -> u64 {
        3
    }

    fn verification_polling_delay_seconds(&self) -> u64 {
        30
    }
}
