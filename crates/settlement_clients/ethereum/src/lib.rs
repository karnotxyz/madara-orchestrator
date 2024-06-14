#![allow(missing_docs)]
#![allow(clippy::missing_docs_in_private_items)]
use alloy::rpc::client::RpcClient;
use alloy::transports::http::Http;
use async_trait::async_trait;
use color_eyre::Result;
use reqwest::Client;
use starknet::core::types::FieldElement;
use std::str::FromStr;
use url::Url;

use config::EthereumSettlementConfig;
use settlement_client_interface::{SettlementClient, SettlementVerificationStatus};

pub mod config;
pub struct EthereumSettlementClient {
    #[allow(dead_code)]
    provider: RpcClient<Http<Client>>,
}

#[async_trait]
impl SettlementClient for EthereumSettlementClient {
    
    async fn register_proof(&self, _proof: Vec<FieldElement>) -> Result<String> {
        unimplemented!()
    }

    async fn update_state_calldata(
        &self,
        _program_output: Vec<FieldElement>,
        _onchain_data_hash: FieldElement,
        _onchain_data_size: FieldElement,
    ) -> Result<String> {
        unimplemented!()
    }

    async fn update_state_blobs(&self, _program_output: Vec<FieldElement>, _kzg_proof: Vec<u8>) -> Result<String> {
        unimplemented!()
    }

    async fn verify_inclusion(&self, _external_id: &str) -> Result<SettlementVerificationStatus> {
        todo!()
    }
}

impl From<EthereumSettlementConfig> for EthereumSettlementClient {
    fn from(config: EthereumSettlementConfig) -> Self {
        let provider = RpcClient::builder()
            .reqwest_http(Url::from_str(config.rpc_url.as_str()).expect("Failed to parse ETHEREUM_RPC_URL"));
        EthereumSettlementClient { provider }
    }
}
