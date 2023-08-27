pub const WS_PROVIDER: &str = "ws://localhost:8545";
pub const HTTP_PROVIDER: &str = "http://localhost:8545";
pub const SHANGHAI_FORK: u64 = 17034870;
pub const DB_PATH: &str = "sqlite://statistics.sqlite";

// -- sled db constants
pub const SLED_DB_PATH: &str = "data";
// seld tree constants
pub const METADATA_TREE: &str = "metadata";
pub const TX_CONTRACT_ADDRESS_TREE: &str = "tx_contract_address";
pub const INIT_CODE_TREE: &str = "init_code";
pub const CONTRACT_TREE: &str = "contract";
// sled key constants
pub const LATEST_BLOCK_NUMBER: &str = "latest_block_number";
