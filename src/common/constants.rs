pub static PROJECT_NAME: &str = "MEV_Bot_Solana";

pub fn get_env(key: &str) -> String {
    std::env::var(key).unwrap_or(String::from(""))
}

#[derive(Debug, Clone)]
pub struct Env {
    pub block_engine_url: String,
    pub mainnet_rpc_url: String,
    pub rpc_url_tx: String,
    pub devnet_rpc_url: String,
    pub rpc_url: String,
    pub wss_rpc_url: String,
    pub geyser_url: String,
    pub geyser_access_token: String,
    pub simulator_url: String,
    pub ws_simulator_url: String,
    pub payer_keypair_path: String,
    pub database_name: String,
    pub max_retries: Option<usize>,
    pub send_retry_count: Option<usize>,
    pub priority_fee_lut: Option<u64>,
    pub max_retries_lut: Option<usize>,
    pub lut_buffer_count: Option<u64>,
}

impl Default for Env {
    fn default() -> Self {
        Self::new()
    }
}

impl Env {
    pub fn new() -> Self {
        Env {
            block_engine_url: get_env("BLOCK_ENGINE_URL"),
            rpc_url: get_env("RPC_URL"),
            mainnet_rpc_url: get_env("MAINNET_RPC_URL"),
            rpc_url_tx: get_env("RPC_URL_TX"),
            devnet_rpc_url: get_env("DEVNET_RPC_URL"),
            wss_rpc_url: get_env("WSS_RPC_URL"),
            geyser_url: get_env("GEYSER_URL"),
            geyser_access_token: get_env("GEYSER_ACCESS_TOKEN"),
            simulator_url: get_env("SIMULATOR_URL"),
            ws_simulator_url: get_env("WS_SIMULATOR_URL"),
            payer_keypair_path: get_env("PAYER_KEYPAIR_PATH"),
            database_name: get_env("DATABASE_NAME"),
            max_retries: get_env("MAX_RETRIES").parse().ok(),
            send_retry_count: get_env("SEND_RETRY_COUNT").parse().ok(),
            priority_fee_lut: get_env("PRIORITY_FEE_LUT").parse().ok(),
            max_retries_lut: get_env("MAX_RETRIES_LUT").parse().ok(),
            lut_buffer_count: get_env("LUT_BUFFER_COUNT").parse().ok(),
        }
    }
}
