use crate::{
    common::ethereum,
    common::{api::ResponseError, Coin, Timestamp, WalletAddress},
    transactions::QuoteTx,
    utils::bip44,
    utils::price,
    vault::{
        config::{NetType, VAULT_CONFIG},
        processor::utils::get_swap_expire_timestamp,
        transactions::TransactionProvider,
    },
};
use parking_lot::RwLock;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

mod validation;

/// Params for the v1/quote endpoint
#[serde(rename_all = "camelCase")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuoteParams {
    input_coin: Coin,
    input_return_address: Option<String>,
    input_address_id: String,
    input_amount: String, // Amounts are strings,
    output_coin: Coin,
    output_address: String,
    slippage_limit: f32,
}

/// Response for the v1/quote endpoint
#[serde(rename_all = "camelCase")]
#[derive(Debug, Deserialize, Serialize)]
pub struct QuoteResponse {
    id: Uuid,         // unique id
    created_at: u128, // milliseconds from epoch
    expires_at: u128, // milliseconds from epoch
    input_coin: Coin,
    input_address: String,                // Generated on the server,
    input_return_address: Option<String>, // User specified address,
    input_amount: String,
    output_coin: Coin,
    output_address: String,
    estimated_output_amount: String, // Generated on the server. Quoted amount.
    slippage_limit: f32,
}

fn bad_request(message: &str) -> ResponseError {
    ResponseError::new(StatusCode::BAD_REQUEST, message)
}

fn internal_server_error() -> ResponseError {
    ResponseError::new(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error")
}

fn generate_bip44_keypair_from_root_key(
    root_key: &str,
    coin: bip44::CoinType,
    index: u64,
) -> Result<bip44::KeyPair, String> {
    let root_key = bip44::RawKey::decode(root_key).map_err(|err| format!("{}", err))?;

    let root_key = root_key
        .to_private_key()
        .ok_or("Failed to generate extended private key".to_owned())?;

    let key_pair = bip44::get_key_pair(root_key, coin, index)?;

    return Ok(key_pair);
}

fn generate_eth_address(root_key: &str, index: u64) -> Result<String, String> {
    let key_pair =
        generate_bip44_keypair_from_root_key(root_key, bip44::CoinType::ETH, index).unwrap();

    Ok(ethereum::Address::from(key_pair.public_key).to_string())
}

fn generate_btc_address(
    root_key: &str,
    index: u64,
    compressed: bool,
    address_type: bitcoin::AddressType,
    nettype: &NetType,
) -> Result<String, String> {
    let key_pair = generate_bip44_keypair_from_root_key(root_key, bip44::CoinType::BTC, index)?;
    let btc_pubkey = bitcoin::PublicKey {
        key: key_pair.public_key,
        compressed,
    };

    let network = match nettype {
        NetType::Testnet => bitcoin::Network::Testnet,
        NetType::Mainnet => bitcoin::Network::Bitcoin,
    };

    let address = match address_type {
        bitcoin::AddressType::P2wpkh => bitcoin::Address::p2wpkh(&btc_pubkey, network),
        bitcoin::AddressType::P2pkh => bitcoin::Address::p2pkh(&btc_pubkey, network),
        _ => {
            warn!(
                "Address type of {} is not currently supported. Defaulting to p2wpkh address",
                address_type
            );
            bitcoin::Address::p2wpkh(&btc_pubkey, network)
        }
    };
    let address = address.to_string();

    Ok(address)
}

/// Request a swap quote
pub async fn post_quote<T: TransactionProvider>(
    params: QuoteParams,
    provider: Arc<RwLock<T>>,
) -> Result<QuoteResponse, ResponseError> {
    let original_params = params.clone();

    if let Err(err) = validation::validate_params(&params) {
        return Err(bad_request(err));
    }

    // Validation of these should have been handled above
    let input_coin = params.input_coin;
    let output_coin = params.output_coin;
    let input_amount = params
        .input_amount
        .parse::<u128>()
        .map_err(|_| internal_server_error())?;

    let mut provider = provider.write();
    provider.sync();

    // Ensure we don't have a quote with the address
    if let Some(_) = provider.get_quote_txs().iter().find(|quote_info| {
        let quote = &quote_info.inner;
        quote.input == input_coin && quote.input_address_id == params.input_address_id
    }) {
        return Err(bad_request("Quote already exists for input address id"));
    }

    // Calculate the output amount
    let estimated_output_amount =
        price::get_output(&(*provider), input_coin, input_amount, output_coin)
            .map(|calculation| {
                let detail = calculation.second.unwrap_or(calculation.first);
                detail.output_amount
            })
            .unwrap_or(0);
    if estimated_output_amount == 0 {
        return Err(bad_request("Not enough liquidity"));
    }

    let effective_price = input_amount as f64 / estimated_output_amount as f64;

    // Generate addresses
    let input_address = match input_coin {
        Coin::ETH => {
            let index = match params.input_address_id.parse::<u64>() {
                Ok(index) => index,
                Err(_) => return Err(bad_request("Incorrect input address id")),
            };
            match generate_eth_address(&VAULT_CONFIG.eth.master_root_key, index) {
                Ok(address) => address,
                Err(err) => {
                    warn!("Failed to generate ethereum address: {}", err);
                    return Err(internal_server_error());
                }
            }
        }
        Coin::LOKI => {
            // TODO: Generate integrated address here
            "T6SMsepawgrKXeFmQroAbuTQMqLWyMxiVUgZ6APCRFgxQAUQ1AkEtHxAgDMZJJG9HMJeTeDsqWiuCMsNahScC7ZS2StC9kHhY".into()
        }
        Coin::BTC => {
            let index = match params.input_address_id.parse::<u64>() {
                Ok(index) => index,
                Err(_) => return Err(bad_request("Incorrect input address id")),
            };
            match generate_btc_address(
                &VAULT_CONFIG.btc.master_root_key,
                index,
                false,
                bitcoin::AddressType::P2wpkh,
                &VAULT_CONFIG.net_type,
            ) {
                Ok(address) => address,
                Err(err) => {
                    warn!("Failed to generate bitcoin address: {}", err);
                    return Err(internal_server_error());
                }
            }
        }
        _ => {
            warn!(
                "Input address generation not implemented for {}",
                input_coin
            );
            return Err(internal_server_error());
        }
    };

    let quote = QuoteTx::new(
        Timestamp::now(),
        input_coin,
        WalletAddress::new(&input_address),
        params.input_address_id,
        params.input_return_address.clone().map(WalletAddress),
        output_coin,
        WalletAddress::new(&params.output_address),
        effective_price,
        params.slippage_limit,
    )
    .map_err(|err| {
        error!(
            "Failed to create quote tx for params: {:?} due to error {}",
            original_params,
            err.clone()
        );
        bad_request(err)
    })?;

    provider
        .add_transactions(vec![quote.clone().into()])
        .map_err(|err| {
            error!("Failed to add quote transaction: {}", err);
            internal_server_error()
        })?;

    Ok(QuoteResponse {
        id: quote.id,
        created_at: quote.timestamp.0,
        expires_at: get_swap_expire_timestamp(&quote.timestamp).0,
        input_coin,
        input_address: input_address.to_string(),
        input_return_address: params.input_return_address,
        input_amount: params.input_amount,
        output_coin,
        output_address: params.output_address,
        estimated_output_amount: estimated_output_amount.to_string(),
        slippage_limit: params.slippage_limit,
    })
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        common::coins::PoolCoin, transactions::PoolChangeTx,
        utils::test_utils::get_transactions_provider,
    };
    use bitcoin::AddressType::*;

    // NEVER USE THIS IN AN ACTUAL APPLICATION! ONLY FOR TESTING
    const ROOT_KEY: &str = "xprv9s21ZrQH143K3sFfKzYqgjMWgvsE44f6gxaRvyo11R22u2p5qegToQaEi7e6e5mRq3f92g9yDQQtu488ggct5gUspippg678t1QTCwBRb85";

    fn params() -> QuoteParams {
        QuoteParams {
            input_coin: Coin::LOKI,
            input_return_address: Some("T6SMsepawgrKXeFmQroAbuTQMqLWyMxiVUgZ6APCRFgxQAUQ1AkEtHxAgDMZJJG9HMJeTeDsqWiuCMsNahScC7ZS2StC9kHhY".to_string()),
            input_address_id: "60900e5603bf96e3".to_owned(),
            input_amount: "1000000000".to_string(),
            output_coin: Coin::ETH,
            output_address: "0x70e7db0678460c5e53f1ffc9221d1c692111dcc5".to_string(),
            slippage_limit: 0.0,
        }
    }

    #[test]
    fn generates_correct_eth_address() {
        assert_eq!(
            &generate_eth_address(ROOT_KEY, 0).unwrap(),
            "0x48575a3C8fa7D0469FD39eCB67ec68d8C7564637"
        );
        assert_eq!(
            &generate_eth_address(ROOT_KEY, 1).unwrap(),
            "0xB46878bd2E68e2b3f5145ccB868E626572905c5F"
        );
    }

    #[test]
    fn generates_correct_btc_address() {
        // === p2wpkh - pay-to-witness-pubkey-hash (segwit) addresses ===
        assert_eq!(
            generate_btc_address(ROOT_KEY, 0, false, P2wpkh, &NetType::Mainnet).unwrap(),
            "bc1ql40fhzrdmljydema5mz5hmja7mul8smmdpvjxl"
        );

        // testnet generates different addresses to mainnet
        assert_eq!(
            generate_btc_address(ROOT_KEY, 0, false, P2wpkh, &NetType::Testnet).unwrap(),
            "tb1ql40fhzrdmljydema5mz5hmja7mul8smm88hpav"
        );

        assert_eq!(
            generate_btc_address(ROOT_KEY, 1, false, P2wpkh, &NetType::Mainnet).unwrap(),
            "bc1q7mlzxxwdx6ut660sg6fs8yhz3tphv6r28rwr3m"
        );

        // === p2pkh - pay-to-pubkey-hash (legacy) addresses ===
        assert_eq!(
            generate_btc_address(ROOT_KEY, 0, false, P2pkh, &NetType::Mainnet).unwrap(),
            "1Q6hHytu6sZmib3TUNeEhGxE8L2ydx5JZo",
        );

        // testnet generates different addresses to mainnet
        assert_eq!(
            generate_btc_address(ROOT_KEY, 0, false, P2pkh, &NetType::Testnet).unwrap(),
            "n4ceb2ysuu12VhX5BwccXCAYzKdgZY2XFH",
        );

        assert_eq!(
            generate_btc_address(ROOT_KEY, 1, true, P2pkh, &NetType::Mainnet).unwrap(),
            "1LbqQTsn9EJN1yWJ2YkQGtaihovjgs6cfW"
        );

        assert_eq!(
            generate_btc_address(ROOT_KEY, 1, false, P2pkh, &NetType::Mainnet).unwrap(),
            "1PWyfwtkS9co1rTHvU2SSESbcu6zi2TmxH"
        );

        assert_ne!(
            generate_btc_address(ROOT_KEY, 2, false, P2pkh, &NetType::Mainnet).unwrap(),
            "1LbqQTsn9EJN1yWJ2YkQGtaihovjgs6cfW"
        );

        assert!(generate_btc_address("not a real key", 4, false, P2pkh, &NetType::Mainnet).is_err())
    }

    #[tokio::test]
    async fn returns_error_if_quote_exists() {
        let quote_params = params();

        let mut provider = get_transactions_provider();
        let quote = QuoteTx {
            id: Uuid::new_v4(),
            timestamp: Timestamp::now(),
            input: quote_params.input_coin,
            input_address: WalletAddress::new("T6SMsepawgrKXeFmQroAbuTQMqLWyMxiVUgZ6APCRFgxQAUQ1AkEtHxAgDMZJJG9HMJeTeDsqWiuCMsNahScC7ZS2StC9kHhY"),
            input_address_id: quote_params.input_address_id,
            return_address: quote_params.input_return_address.clone().map(WalletAddress),
            output: quote_params.output_coin,
            slippage_limit: quote_params.slippage_limit,
            output_address: WalletAddress::new(&quote_params.output_address),
            effective_price: 1.0
        };
        provider.add_transactions(vec![quote.into()]).unwrap();

        let provider = Arc::new(RwLock::new(provider));

        let result = post_quote(params(), provider)
            .await
            .expect_err("Expected post_quote to return error");

        assert_eq!(&result.message, "Quote already exists for input address id");
    }

    #[tokio::test]
    async fn returns_error_if_no_liquidity() {
        let provider = get_transactions_provider();
        let provider = Arc::new(RwLock::new(provider));

        // No pools yet
        let result = post_quote(params(), provider.clone())
            .await
            .expect_err("Expected post_quote to return error");

        assert_eq!(&result.message, "Not enough liquidity");

        // Pool with no liquidity
        {
            let tx = PoolChangeTx::new(PoolCoin::ETH, 0, 0);

            let mut provider = provider.write();
            provider.add_transactions(vec![tx.into()]).unwrap();
        }

        let result = post_quote(params(), provider.clone())
            .await
            .expect_err("Expected post_quote to return error");

        assert_eq!(&result.message, "Not enough liquidity");
    }

    #[tokio::test]
    async fn returns_response_if_successful() {
        let mut provider = get_transactions_provider();
        let tx = PoolChangeTx::new(PoolCoin::ETH, 10_000_000_000, 50_000_000_000);
        provider.add_transactions(vec![tx.into()]).unwrap();

        let provider = Arc::new(RwLock::new(provider));

        post_quote(params(), provider.clone())
            .await
            .expect("Expected to get a quote response");

        assert_eq!(provider.read().get_quote_txs().len(), 1);
    }
}
