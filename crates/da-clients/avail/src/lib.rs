#![allow(missing_docs)]
#![allow(clippy::missing_docs_in_private_items)]
use async_trait::async_trait;
use avail_subxt::{AvailClient, tx};
use avail_subxt::api::data_availability::calls::types::SubmitData;
use avail_subxt::api::runtime_types::avail_core::AppId;
use avail_subxt::primitives::CheckAppId;
use avail_subxt::submit::submit_data_with_nonce;
use avail_subxt::utils::H256;
use color_eyre::eyre;
use color_eyre::eyre::{anyhow, eyre};
use mockall::automock;
use subxt::ext::sp_core::sha2_256;
use subxt_signer::sr25519::{Keypair, SecretKeyBytes};

use config::AvailDaConfig;
use da_client_interface::{DaClient, DaVerificationStatus};

type AvailSigner = Keypair;

pub mod config;

pub struct AvailDaClient {
    client: AvailClient,
    app_id: AppId,
    signer: Keypair,
}

#[automock]
#[async_trait]
impl DaClient for AvailDaClient {
    async fn publish_state_diff(&self, state_diff: Vec<Vec<u8>>, _to: &[u8; 32]) -> eyre::Result<String> {
        let client = &self.client;
        let signer = &self.signer;
        let nonce = tx::nonce(&client, &signer)
            .await
            .expect("Failed to get nonce.");
        let data = get_bytes_from_state_diff(state_diff);
        let state_diff_hash: H256 = sha2_256(&data).into();
        let tx_progress = submit_data_with_nonce(&client, &signer, &data, &self.app_id, nonce)
            .await.expect("Failed to submit data.");

        // Must wait here to get the block hash of the transaction.
        let block_hash = tx::then_in_block(tx_progress)
            .await
            .expect("Failed to get block hash.")
            .block_hash();

        // Return the statediff hash and block hash, since there is no way to get the transaction with extrinsic hash.
        Ok(format!("{}:{}", state_diff_hash.to_string(), block_hash.to_string()))
    }

    #[allow(unused)]
    async fn verify_inclusion(&self, external_id: &str) -> eyre::Result<DaVerificationStatus> {
        let client = &self.client;

        let mut parts = external_id.split(':');
        if(parts.len() != 2) {
            return eyre!("Invalid external id format: {}", external_id)
        }
        let data_hash: H256 = parts.next().unwrap_or("").parse();
        let block_hash = parts.next().unwrap_or("");

        let block = client
            .blocks()
            .at(block_hash)
            .await
            .expect("Failed to get block.");
        let extrinsics = block.extrinsics().await.expect("Failed to get extrinsics.");
        let mut found = false;
        let mut tx_idx = 0;
        let da_submissions = extrinsics.find::<SubmitData>();
        let mut found = false;
        for da_submission in da_submissions {
            let da_submission = da_submission?;
            let call = da_submission.as_extrinsic::<SubmitData>();
            if let Ok(Some(call)) = call {

                if data_hash.to_string() == sha2_256(call.data.0).into().to_string() {
                    found = true;
                    break;
                }
            }
            tx_idx += 1;
        }

        if !found {
            Ok(DaVerificationStatus::Rejected)
        }

        Ok(DaVerificationStatus::Verified)
    }

    async fn max_blob_per_txn(&self) -> u64 {
        // TODO: Avail is not limited by the number of blobs per transaction.
        6
    }

    async fn max_bytes_per_blob(&self) -> u64 {
        // TODO: Avail is not limited max bytes per blob. I got this number from Zksync hyperchain.
        512 * 1024
    }
}

pub fn try_create_avail_client(rpc_url: &str) -> eyre::Result<AvailClient> {
    let client =
        futures::executor::block_on(async { AvailClient::new(rpc_url).await })
            .expect("Failed to create client.");
    Ok(client)
}

pub fn get_bytes_from_state_diff(state_diff: Vec<Vec<u8>>) -> Vec<u8> {
    // TODO: This is a dummy implementation. We need to implement a proper way to convert state diff to bytes.
    state_diff.into_iter().flatten().collect()
}

impl AvailDaClient {
    #[allow(dead_code)]
    fn from(config: AvailDaConfig) -> eyre::Result<Self> {
        let client = try_create_avail_client(&config.rpc_url).expect("Failed to parse AVAIL_RPC_URL.");
        let signer = Keypair::from_secret_key(SecretKeyBytes::from_hex(config.private_key.clone())).expect("Failed to create keypair.");
        let app_id = AppId(config.app_id.parse().expect("Failed to parse app id."));
        Ok(AvailDaClient { client, app_id, signer })
    }
}

#[cfg(test)]
mod tests {

}
