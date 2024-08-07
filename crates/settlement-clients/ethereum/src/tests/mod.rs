use alloy::{hex, node_bindings::Anvil, primitives::{ U256}, providers::Provider};
use color_eyre::eyre::eyre;
use rstest::*;
use settlement_client_interface::SettlementClient;
use utils::settings::default::DefaultSettingsProvider;
use std::{fs::{self, File}, io::BufReader, str::FromStr};
use crate::EthereumSettlementClient;
use alloy::providers::{ext::AnvilApi, ProviderBuilder};
use alloy_primitives::Address;
use alloy_primitives::FixedBytes;

use color_eyre::Result;

use std::io::{BufRead};

fn hex_string_to_u8_vec(hex_str: &str) -> color_eyre::Result<Vec<u8>> {
  // Remove any spaces or non-hex characters from the input string
  let cleaned_str: String = hex_str.chars().filter(|c| c.is_ascii_hexdigit()).collect();

  // Convert the cleaned hex string to a Vec<u8>
  let mut result = Vec::new();
  for chunk in cleaned_str.as_bytes().chunks(2) {
      if let Ok(byte_val) = u8::from_str_radix(std::str::from_utf8(chunk)?, 16) {
          result.push(byte_val);
      } else {
          return Err(eyre!("Error parsing hex string: {}", cleaned_str));
      }
  }

  Ok(result)
}

#[rstest]
#[tokio::test]
#[case::basic(20468828)]
async fn update_state_blob_works(#[case] block_no : u64) {
  // Only Supports Ethereum Blocks

  dotenvy::from_filename("../.env.test").expect("Could not load .env.test file");

  // let anvil = Anvil::new().port(3000_u16).fork("https://eth.llamarpc.com").fork_block_number(block_no - 1).try_spawn()
  // .expect("Unable to spawn Anvil");
  use url::Url;

  // // https://github.dev/alloy-rs/alloy
  let provider = ProviderBuilder::new().on_http(Url::from_str("http://localhost:3000").expect("dskj"));
  // // provider.anvil_auto_impersonate_account(false).await.unwrap();
  
  // // let gas = U256::from(1337);
  // // provider.anvil_set_min_gas_price(gas).await.expect("could not set min gas ");
  
  // println!("BASE GAS PRICE : {}",provider.get_blob_base_fee().await.expect("could not get base gas price"));
  
  // // provider.anvil_set_balance(Address::from_str("0x6E9972213BF459853FA33E28Ab7219e9157C8d02").expect("lol"), U256::from(1000)).await.expect("couldn't set balance");
  // provider.anvil_set_balance(Address::from_str("0x2C169DFe5fBbA12957Bdd0Ba47d9CEDbFE260CA7").expect("lol"), U256::from(1000000000)).await.expect("couldn't set balance");
  provider.anvil_impersonate_account(Address::from_str("0x2C169DFe5fBbA12957Bdd0Ba47d9CEDbFE260CA7").expect("sdjkvb")).await.expect("sdcjb");
  println!("Balance : {}", provider.get_balance(Address::from_str("0x2C169DFe5fBbA12957Bdd0Ba47d9CEDbFE260CA7").expect("sdjkvb")).await.expect("could not get balance"));



  // println!("Anvil running at `{}`", anvil.endpoint());

  let settings_provider: DefaultSettingsProvider = DefaultSettingsProvider {};
  let ethereum_settlement_client = EthereumSettlementClient::with_settings(&settings_provider);

  let current_path = std::env::current_dir().unwrap().to_str().unwrap().to_string();

  // Program Output
  let program_output_file_path =
  format!("{}{}{}{}", current_path.clone(), "/src/test_data/program_output/", block_no, ".txt");
  println!("{}", program_output_file_path);
  
  let mut program_output : Vec<[u8;32]> = Vec::new(); 
  {
    let file = File::open(program_output_file_path)
    .expect("can't read file");
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = line
        .expect("can't read line");
        let trimmed = line.trim();
        if !trimmed.is_empty() {
          let line_u8_32: [u8; 32] =  U256::from_str(trimmed).expect("unable to convert line").to_le_bytes();
          program_output.push(line_u8_32);
        }
    }
  }

  // Blob Data
  let blob_data_file_path =
  format!("{}{}{}{}", current_path.clone(), "/src/test_data/blob_data/", block_no, ".txt");
  println!("{}", blob_data_file_path);
  let blob_data = fs::read_to_string(blob_data_file_path).expect("Failed to read the blob data txt file");
  let blob_data_vec = vec![hex_string_to_u8_vec(&blob_data).unwrap()];

  // Sending transaction
  let update_state_result = ethereum_settlement_client
  .update_state_with_blobs(program_output,blob_data_vec).await
  .expect("update_state_with_blobs failed");

  println!("{}", update_state_result);  
  assert!(!update_state_result.is_empty(), "No Transaction Hash");
  let txn = provider.get_transaction_by_hash(FixedBytes::from_str(update_state_result.as_str()).expect("couln't convert")).await.expect("did not get txn from hash");



  if let Some(txxn) = txn {
    println!("{:?}",txxn);

    // println!("{}",txxn.hash.to_string());
    // println!("{}",txxn.from.to_string());

    // let dsd = provider.get_transaction_receipt(FixedBytes::from_str(update_state_result.as_str()).expect("vdf")).await.expect(":vdd");
    // println!(" reciept {:?}",dsd);
    
    // println!("{:?}",txxn.blob_versioned_hashes);
    // println!("Balance : {}", provider.get_balance(Address::from_str("0x2C169DFe5fBbA12957Bdd0Ba47d9CEDbFE260CA7").expect("sdjkvb")).await.expect("could not get balance"));


  }
}