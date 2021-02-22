use chainflip::{
    common::GenericCoinAmount,
    utils::{bip44::KeyPair, primitives::U256},
    vault::blockchain_connection::ethereum::{
        web3::Web3Client, EstimateRequest, EthereumClient, SendTransaction,
    },
};
use chainflip_common::types::{addresses::EthereumAddress, coin::Coin};
use config::Config;
use serde::Deserialize;
use std::str::FromStr;

/**
Binary for testing eth client.
    Steps to run:
        1. Create `local.toml` under `config/eth-test`. You can use `local.example.toml` as a reference.
        2. Create a new eth account and keep track of the keys (This will be the account that will send ETH)
        3. Go to https://faucet.dimensions.network/ and get some Ropsten ETH
        4. Create another eth account and record its address (This will be the account that you receive the ETH to)
        5. Set information in `local.toml` along with a ropsten provider (See Providers below)
        6. Remove test ignore
        7. Run `cargo test --package chainflip --test integration_test -- eth_web3_client::test_web3_send --exact --color always --nocapture`

Providers:
    It is reccomended you use a local geth light node for testing consistency.
    You can run a local node using the following command:
    ```
    geth --testnet removedb
    geth --testnet --syncmode light --http --http.port 8545 --http.api "eth,web3,personal" --bootnodes "enode://6332792c4a00e3e4ee0926ed89e0d27ef985424d97b6a45bf0f23e51f0dcb5e66b875777506458aea7af6f9e4ffb69f43f3778ee73c81ed9d34c51c4b16b0b0f@52.232.243.152:30303,enode://94c15d1b9e2fe7ce56e458b9a3b672ef11894ddedd0c6f247e0f1d3487f52b66208fb4aeb8179fce6e3a749ea93ed147c37976d67af557508d199d9594c35f09@192.81.208.223:30303"
    ```
    Then set the provider to: http://localhost:8545

    You can also use https://infura.io/.
    Other providers have been tested but they have a problem with getting transaction counts for an account and thus new transactions fail to send because of conflicting nonce values.
*/

#[derive(Debug, Deserialize, Clone)]
/// Configutation for ethereum
pub struct EthConfig {
    /// The seed to derive wallets from
    pub provider: String,
    /// The key of the sender
    pub sender_private_key: String,
    /// The address that will receive the funds
    pub receiving_address: String,
}

impl EthConfig {
    fn from_file() -> Self {
        let mut config = Config::new();
        config
            .merge(config::File::with_name("config/eth-test/local"))
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
#[ignore = "Custom environment setup needed"]
async fn test_web3_send() {
    let config = EthConfig::from_file();
    let web3 = Web3Client::url(&config.provider).unwrap();

    let key_pair = KeyPair::from_private_key(&config.sender_private_key).unwrap();
    let to = EthereumAddress::from_str(&config.receiving_address).unwrap();

    let total_amount = GenericCoinAmount::from_decimal_string(Coin::ETH, "0.005");

    println!("Total amount to send: {}", total_amount.to_atomic());

    let request = EstimateRequest {
        from: EthereumAddress::from_public_key(key_pair.public_key.serialize_uncompressed()),
        to,
        amount: total_amount,
    };

    let estimate = web3.get_estimated_fee(&request).await.unwrap();
    println!("{:?}", estimate);

    let fee = U256::from(estimate.gas_limit)
        .saturating_mul(estimate.gas_price.into())
        .as_u128();

    println!("Fee: {}", fee);

    let new_amount = total_amount.to_atomic() - fee;

    println!("Amount to send: {}", new_amount);

    let transaction = SendTransaction {
        from: key_pair,
        to,
        amount: GenericCoinAmount::from_atomic(Coin::ETH, new_amount),
        gas_limit: estimate.gas_limit,
        gas_price: estimate.gas_price,
    };

    match web3.send(&transaction).await {
        Ok(hash) => println!("Created first tx {}", hash),
        Err(err) => panic!("Failed to send first eth transaction: {}", err),
    };

    match web3.send(&transaction).await {
        Ok(hash) => println!("Created second tx {}", hash),
        Err(err) => panic!("Failed to send second eth transaction: {}", err),
    };
}
