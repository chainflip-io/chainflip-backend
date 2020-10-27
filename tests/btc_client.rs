use bitcoincore_rpc::{self, Auth};
use blockswap::{
    common::{coins::GenericCoinAmount, Coin},
    utils::bip44::KeyPair,
    vault::blockchain_connection::btc::{btc::BtcClient, BitcoinClient, SendTransaction},
};
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

#[tokio::test]
#[ignore = "Custom environment setup required"]
async fn test_btc_send() {
    let config = BtcConfig::from_file();
    let auth = Auth::UserPass(config.rpc_user, config.rpc_password);
    let client = BtcClient::new_from_url_auth(&config.provider, auth).unwrap();

    let key_pair = KeyPair::from_private_key(&config.sender_private_key).unwrap();

    let btc_pubkey = bitcoin::PublicKey {
        key: bitcoin::secp256k1::PublicKey::from_slice(&key_pair.public_key.serialize()).unwrap(),
        compressed: false,
    };

    // we're going to send it to ourselves
    let to = bitcoin::Address::p2pkh(&btc_pubkey, client.get_network_type());

    let total_amount = GenericCoinAmount::from_atomic(Coin::BTC, 1000);

    // amount with or without fees? This is defined in the client send, using the
    // subtract_fee option
    let send_tx = SendTransaction {
        from: key_pair,
        to,
        amount: total_amount,
    };

    match client.send(&send_tx).await {
        Ok(txid) => println!("Sent the btc transaction: {}", txid),
        Err(err) => panic!("Failed to send the btc transaction: {}", err),
    };
}
