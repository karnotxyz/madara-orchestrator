use da_client_interface::DaConfig;
use serde::Deserialize;
use utils::env_utils::get_env_var_or_panic;

#[derive(Clone, PartialEq, Deserialize, Debug)]
pub struct AvailDaConfig {
    pub rpc_url: String,
    pub app_id: String,
    pub private_key: String,
}

impl DaConfig for AvailDaConfig {
    fn new_from_env() -> Self {
        Self {
            rpc_url: get_env_var_or_panic("AVAIL_RPC_URL"),
            app_id: get_env_var_or_panic("AVAIL_APP_ID"),
            private_key: get_env_var_or_panic("AVAIL_PRIVATE_KEY"),
        }
    }
}
