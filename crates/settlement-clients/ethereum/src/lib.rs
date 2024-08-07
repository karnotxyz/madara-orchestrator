pub mod clients;
pub mod config;
pub mod conversion;
pub mod types;
mod tests;

use alloy::consensus::{
    BlobTransactionSidecar, SignableTransaction, TxEip4844, TxEip4844Variant, TxEip4844WithSidecar, TxEnvelope,
};
use alloy::eips::eip2718::Encodable2718;
use alloy::eips::eip2930::AccessList;
use alloy::eips::eip4844::BYTES_PER_BLOB;
use alloy::hex;
use alloy::network::TransactionBuilder;
use alloy::providers::ext::AnvilApi;
use alloy::rpc::types::TransactionRequest;
use alloy::{
    network::EthereumWallet,
    primitives::{Address, B256, U256},
    providers::{PendingTransactionConfig, Provider, ProviderBuilder},
    rpc::types::TransactionReceipt,
    signers::local::PrivateKeySigner,
};
use alloy_primitives::Bytes;
use async_trait::async_trait;
use c_kzg::{Blob, Bytes32, KzgCommitment, KzgProof, KzgSettings};
use color_eyre::eyre::eyre;
use color_eyre::Result;
use conversion::{get_txn_input_bytes, prepare_sidecar};
use mockall::{automock, lazy_static, predicate::*};

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use crate::clients::interfaces::validity_interface::StarknetValidityContractTrait;
use settlement_client_interface::{SettlementClient, SettlementVerificationStatus, SETTLEMENT_SETTINGS_NAME};
use utils::{env_utils::get_env_var_or_panic, settings::SettingsProvider};

use crate::clients::StarknetValidityContractClient;
use crate::config::EthereumSettlementConfig;
use crate::conversion::{slice_slice_u8_to_vec_u256, slice_u8_to_u256};
use crate::types::EthHttpProvider;

pub const ENV_PRIVATE_KEY: &str = "ETHEREUM_PRIVATE_KEY";

lazy_static! {
    pub static ref CURRENT_PATH: PathBuf = std::env::current_dir().unwrap();
    pub static ref KZG_SETTINGS: KzgSettings = KzgSettings::load_trusted_setup_file(
        CURRENT_PATH.join("src/trusted_setup.txt").as_path()
    )
    .expect("Error loading trusted setup file");
}

#[allow(dead_code)]
pub struct EthereumSettlementClient {
    provider: Arc<EthHttpProvider>,
    core_contract_client: StarknetValidityContractClient,
    wallet: EthereumWallet,
    wallet_address: Address,
}

impl EthereumSettlementClient {
    pub fn with_settings(settings: &impl SettingsProvider) -> Self {
        let settlement_cfg: EthereumSettlementConfig = settings.get_settings(SETTLEMENT_SETTINGS_NAME).unwrap();

        let private_key = get_env_var_or_panic(ENV_PRIVATE_KEY);
        let signer: PrivateKeySigner = private_key.parse().expect("Failed to parse private key");
        let wallet_address = signer.address();
        let wallet = EthereumWallet::from(signer);

        let provider = Arc::new(
            ProviderBuilder::new().with_recommended_fillers().wallet(wallet.clone()).on_http(settlement_cfg.rpc_url),
        );
        let core_contract_client = StarknetValidityContractClient::new(
            Address::from_str(&settlement_cfg.core_contract_address)
                .expect("Failed to convert the validity contract address.")
                .0
                .into(),
            provider.clone(),
        );

        EthereumSettlementClient { provider, core_contract_client, wallet, wallet_address }
    }

    /// Build kzg proof for the x_0 point evaluation
    async fn build_proof(blob_data: Vec<Vec<u8>>, x_0_value: Bytes32) -> Result<KzgProof> {
        // Assuming that there is only one blob in the whole Vec<Vec<u8>> array for now.
        // Later we will add the support for multiple blob in single blob_data vec.
        assert_eq!(blob_data.len(), 1);

        let fixed_size_blob: [u8; BYTES_PER_BLOB] = blob_data[0].as_slice().try_into()?;

        let blob = Blob::new(fixed_size_blob);
        let commitment = KzgCommitment::blob_to_kzg_commitment(&blob, &KZG_SETTINGS)?;
        let (kzg_proof, y_0_value) = KzgProof::compute_kzg_proof(&blob, &x_0_value, &KZG_SETTINGS)?;

        // Verifying the proof for double check
        let eval = KzgProof::verify_kzg_proof(
            &commitment.to_bytes(),
            &x_0_value,
            &y_0_value,
            &kzg_proof.to_bytes(),
            &KZG_SETTINGS,
        )?;

        if !eval {
            Err(eyre!("ERROR : Assertion failed, not able to verify the proof."))
        } else {
            Ok(kzg_proof)
        }
    }
}

#[automock]
#[async_trait]
impl SettlementClient for EthereumSettlementClient {
    /// Should register the proof on the base layer and return an external id
    /// which can be used to track the status.
    #[allow(unused)]
    async fn register_proof(&self, proof: [u8; 32]) -> Result<String> {
        todo!("register_proof is not implemented yet")
    }

    /// Should be used to update state on core contract when DA is done in calldata
    async fn update_state_calldata(
        &self,
        program_output: Vec<[u8; 32]>,
        onchain_data_hash: [u8; 32],
        onchain_data_size: usize,
    ) -> Result<String> {
        let program_output: Vec<U256> = slice_slice_u8_to_vec_u256(program_output.as_slice())?;
        let onchain_data_hash: U256 = slice_u8_to_u256(&onchain_data_hash)?;
        let onchain_data_size: U256 = onchain_data_size.try_into()?;
        let tx_receipt =
            self.core_contract_client.update_state(program_output, onchain_data_hash, onchain_data_size).await?;
        Ok(format!("0x{:x}", tx_receipt.transaction_hash))
    }

    /// Should be used to update state on core contract when DA is in blobs/alt DA
    async fn update_state_blobs(&self, program_output: Vec<[u8; 32]>, kzg_proof: [u8; 48]) -> Result<String> {
        let program_output: Vec<U256> = slice_slice_u8_to_vec_u256(&program_output)?;
        let tx_receipt = self.core_contract_client.update_state_kzg(program_output, kzg_proof).await?;
        Ok(format!("0x{:x}", tx_receipt.transaction_hash))
    }

    async fn update_state_with_blobs(&self, program_output: Vec<[u8; 32]>, state_diff: Vec<Vec<u8>>) -> Result<String> {
        let trusted_setup = KzgSettings::load_trusted_setup_file(Path::new("/Users/dexterhv/Work/Karnot/madara-alliance/madara-orchestrator/crates/settlement-clients/ethereum/src/trusted_setup.txt"))
            .expect("issue while loading the trusted setup");
        let (sidecar_blobs, sidecar_commitments, sidecar_proofs) = prepare_sidecar(&state_diff, &trusted_setup).await?;
        let sidecar = BlobTransactionSidecar::new(sidecar_blobs, sidecar_commitments, sidecar_proofs);

        let eip1559_est = self.provider.estimate_eip1559_fees(None).await?;
        let chain_id: u64 = self.provider.get_chain_id().await?.to_string().parse()?;

        let mut max_fee_per_blob_gas: u128 = self.provider.get_blob_base_fee().await?.to_string().parse()?;
        // TODO: need to send more than current gas price.
        max_fee_per_blob_gas+= 12;
        println!("WALLET ADDRESS : {}", self.wallet_address);
        println!("Balance : {}", self.provider.get_balance(self.wallet_address).await.expect("could not get balance"));
        println!("MAX FEE BLOB : {} {}", max_fee_per_blob_gas, eip1559_est.max_fee_per_gas.to_string());
        let max_priority_fee_per_gas: u128 = self.provider.get_max_priority_fee_per_gas().await?.to_string().parse()?;

        let nonce = self.provider.get_transaction_count(self.wallet_address).await?.to_string().parse()?;

        // x_0_value : program_output[6]
        let kzg_proof = Self::build_proof(
            state_diff,
            Bytes32::from_bytes(program_output[6].as_slice()).expect("Not able to get x_0 point params."),
        )
        .await
        .expect("Unable to build KZG proof for given params.")
        .to_owned();

        let tx: TxEip4844 = TxEip4844 {
            chain_id,
            nonce,
            gas_limit: 30_000_000,
            max_fee_per_gas: eip1559_est.max_fee_per_gas.to_string().parse()?,
            max_priority_fee_per_gas,
            to: self.core_contract_client.contract_address(),
            value: U256::from(0),
            access_list: AccessList(vec![]),
            blob_versioned_hashes: sidecar.versioned_hashes().collect(),
            max_fee_per_blob_gas,
            input: Bytes::from(hex::decode("0xb72d42a100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000340000000000000000000000000000000000000000000000000000000000000001706ac7b2661801b4c0733da6ed1d2910b3b97259534ca95a63940932513111fba028bccc051eaae1b9a69b53e64a68021233b4dee2030aeda4be886324b3fbb3e00000000000000000000000000000000000000000000000000000000000a29b8070626a88de6a77855ecd683757207cdd18ba56553dca6c0c98ec523b827bee005ba2078240f1585f96424c2d1ee48211da3b3f9177bf2b9880b4fc91d59e9a2000000000000000000000000000000000000000000000000000000000000000100000000000000002b4e335bc41dc46c71f29928a5094a8c96a0c3536cabe53e0000000000000000810abb1929a0d45cdd62a20f9ccfd5807502334e7deb35d404c86d8b63a5741770fefca2f9b8efb7e663d89097edb3c60595b236f6e78e6f000000000000000000000000000000004a4b8a979fefc4d6b82e030fb082ca98000000000000000000000000000000004e8371c6774260e87b92447d4a2b0e170000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000bf67f59d2988a46fbff7ed79a621778a3cd3985b0088eedbe2fe3918b69ccb411713b7fa72079d4eddf291103ccbe41e78a9615c0000000000000000000000000000000000000000000000000000000000194fe601b64b1b3b690b43b9b514fb81377518f4039cd3e4f4914d8a6bdf01d679fb1900000000000000000000000000000000000000000000000000000000000000050000000000000000000000007f39c581f595b53c5cb19bd0b3f8da6c935e2ca000000000000000000000000012ccc443d39da45e5f640b3e71f0c7502152dbac01d4988e248d342439aa025b302e1f07595f6a5c810dcce23e7379e48f05d4cf000000000000000000000000000000000000000000000007f189b5374ad2a00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000030ab015987628cffee3ef99b9768ef8ca12c6244525f0cd10310046eaa21291b5aca164d044c5b4ad7212c767b165ed5e300000000000000000000000000000000").unwrap()),
        };
        let tx_sidecar = TxEip4844WithSidecar { tx: tx.clone(), sidecar };
        // let mut variant = TxEip4844Variant::from(tx_sidecar);

        // Sign and submit
        let mut txn : TransactionRequest = tx.into();
        // txn.set
        txn = txn.with_from(Address::from_str("0x2C169DFe5fBbA12957Bdd0Ba47d9CEDbFE260CA7").expect("lol"));
        txn.set_blob_sidecar(tx_sidecar.sidecar);
        txn.set_nonce(666068);
        // let signature = self.wallet.default_signer().sign_transaction(&mut variant).await?;

        // let tx_signed = variant.into_signed(signature);
        // let tx_envelope: TxEnvelope = tx_signed.into();
        // let encoded = tx_envelope.encoded_2718();


        // let pending_tx = self.provider.send_raw_transaction(&encoded).await?;
        
        let pending_tx = self.provider.send_transaction(txn).await.expect("dsf");

        // println!(" pending_tx {:?}", pending_tx );

        Ok("0x2b3fb5f9a59c0687e6e33ca0fc2fe7c02be013a52e5935d8a7ec19dbac95d081".into())
    }

    /// Should verify the inclusion of a tx in the settlement layer
    async fn verify_tx_inclusion(&self, tx_hash: &str) -> Result<SettlementVerificationStatus> {
        let tx_hash = B256::from_str(tx_hash)?;
        let maybe_tx_status: Option<TransactionReceipt> = self.provider.get_transaction_receipt(tx_hash).await?;
        match maybe_tx_status {
            Some(tx_status) => {
                if tx_status.status() {
                    Ok(SettlementVerificationStatus::Verified)
                } else {
                    Ok(SettlementVerificationStatus::Pending)
                }
            }
            None => Ok(SettlementVerificationStatus::Rejected(format!("Could not find status of tx: {}", tx_hash))),
        }
    }

    /// Wait for a pending tx to achieve finality
    async fn wait_for_tx_finality(&self, tx_hash: &str) -> Result<()> {
        let tx_hash = B256::from_str(tx_hash)?;
        self.provider.watch_pending_transaction(PendingTransactionConfig::new(tx_hash)).await?;
        Ok(())
    }

    /// Get the last block settled through the core contract
    async fn get_last_settled_block(&self) -> Result<u64> {
        let block_number = self.core_contract_client.state_block_number().await?;
        Ok(block_number.try_into()?)
    }
}
