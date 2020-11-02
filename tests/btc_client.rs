use config::Config;
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
/// Configutation for ethereum
pub struct BtcConfig {
    /// The seed to derive wallets from
    pub provider: String,
    /// The key of the sender
    pub sender_private_key: String,
    /// The address that will receive the funds
    pub receiving_address: String,
    /// rpc username
    pub rpc_user: String,
    /// rpc password
    pub rpc_password: String,
}

impl BtcConfig {
    fn from_file() -> Self {
        let mut config = Config::new();
        config
            .merge(config::File::with_name("config/btc-test/local"))
            .unwrap();

        let config: Self = config.try_into().unwrap();

        if config.sender_private_key.is_empty() {
            panic!("Missing sender private key")
        }

        if config.receiving_address.is_empty() {
            panic!("Missing receiving address")
        }

        config
    }
}
