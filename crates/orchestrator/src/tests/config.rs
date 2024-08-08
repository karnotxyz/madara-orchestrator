use std::sync::Arc;

use crate::config::{
    build_da_client, build_prover_service, build_settlement_client, build_storage_client, config_force_init, Config,
};
use crate::data_storage::DataStorage;
use da_client_interface::DaClient;
use prover_client_interface::ProverClient;
use settlement_client_interface::SettlementClient;
use starknet::providers::jsonrpc::HttpTransport;
use starknet::providers::{JsonRpcClient, Url};
use utils::env_utils::get_env_var_or_panic;
use utils::settings::default::DefaultSettingsProvider;

use crate::database::mongodb::config::MongoDbConfig;
use crate::database::mongodb::MongoDb;
use crate::database::{Database, DatabaseConfig};
use crate::queue::sqs::SqsQueue;
use crate::queue::QueueProvider;

use httpmock::MockServer;

use super::common::drop_database;
// Inspiration : https://rust-unofficial.github.io/patterns/patterns/creational/builder.html
// TestConfigBuilder allows to heavily customise the global configs based on the test's requirement.
// Eg: We want to mock only the da client and leave rest to be as it is, use mock_da_client.

// TestBuilder for Config
pub struct TestConfigBuilder {
    /// The starknet client to get data from the node
    starknet_client: Option<Arc<JsonRpcClient<HttpTransport>>>,
    /// The DA client to interact with the DA layer
    da_client: Option<Box<dyn DaClient>>,
    /// The service that produces proof and registers it onchain
    prover_client: Option<Box<dyn ProverClient>>,
    /// Settlement client
    settlement_client: Option<Box<dyn SettlementClient>>,
    /// The database client
    database: Option<Box<dyn Database>>,
    /// Queue client
    queue: Option<Box<dyn QueueProvider>>,
    /// Storage client
    storage: Option<Box<dyn DataStorage>>,
}

impl Default for TestConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TestConfigBuilder {
    /// Create a new config
    pub fn new() -> TestConfigBuilder {
        TestConfigBuilder {
            starknet_client: None,
            da_client: None,
            prover_client: None,
            settlement_client: None,
            database: None,
            queue: None,
            storage: None,
        }
    }

    pub fn mock_da_client(mut self, da_client: Box<dyn DaClient>) -> TestConfigBuilder {
        self.da_client = Some(da_client);
        self
    }

    pub async fn build(mut self) -> MockServer {
        dotenvy::from_filename("../.env.test").expect("Failed to load the .env file");

        let server = MockServer::start();

        // init starknet client
        if self.starknet_client.is_none() {
            let provider = JsonRpcClient::new(HttpTransport::new(
                Url::parse(format!("http://localhost:{}", server.port()).as_str()).expect("Failed to parse URL"),
            ));
            self.starknet_client = Some(Arc::new(provider));
        }

        // init database
        if self.database.is_none() {
            self.database = Some(Box::new(MongoDb::new(MongoDbConfig::new_from_env()).await));
        }

        // init queue
        if self.queue.is_none() {
            self.queue = Some(Box::new(SqsQueue {}));
        }

        // init the DA client
        if self.da_client.is_none() {
            self.da_client = Some(build_da_client().await);
        }

        let settings_provider = DefaultSettingsProvider {};

        // init the Settings client
        if self.settlement_client.is_none() {
            self.settlement_client = Some(build_settlement_client(&settings_provider).await);
        }

        // init the Prover client
        if self.prover_client.is_none() {
            self.prover_client = Some(build_prover_service(&settings_provider));
        }

        // init the storage client
        if self.storage.is_none() {
            self.storage = Some(build_storage_client().await);
            match get_env_var_or_panic("DATA_STORAGE").as_str() {
                "s3" => self
                    .storage
                    .as_ref()
                    .unwrap()
                    .build_test_bucket(&get_env_var_or_panic("AWS_S3_BUCKET_NAME"))
                    .await
                    .unwrap(),
                _ => panic!("Unsupported Storage Client"),
            }
        }

        // return config and server as tuple
        let config = Config::new(
            self.starknet_client.unwrap(),
            self.da_client.unwrap(),
            self.prover_client.unwrap(),
            self.settlement_client.unwrap(),
            self.database.unwrap(),
            self.queue.unwrap(),
            self.storage.unwrap(),
        );

        config_force_init(config).await;

        drop_database().await.unwrap();

        server
    }
}
