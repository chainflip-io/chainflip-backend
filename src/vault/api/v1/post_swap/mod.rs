use crate::{
    common::{api::ResponseError, Coin, LokiPaymentId, Timestamp, WalletAddress},
    transactions::QuoteTx,
    utils::price,
    vault::{processor::utils::get_swap_expire_timestamp, transactions::TransactionProvider},
};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::{str::FromStr, sync::Arc};
use uuid::Uuid;

use super::{
    utils::{bad_request, generate_btc_address, generate_eth_address, internal_server_error},
    Config,
};

mod validation;

/// Params for the v1/quote endpoint
#[serde(rename_all = "camelCase")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapQuoteParams {
    /// Input coin
    pub input_coin: Coin,
    /// Input return address
    pub input_return_address: Option<String>,
    /// Input address id
    pub input_address_id: String,
    /// Input atomic amount
    pub input_amount: String, // Amounts are strings,
    /// Output coin
    pub output_coin: Coin,
    /// Output address
    pub output_address: String,
    /// Slippage limit
    pub slippage_limit: f32,
}

/// Response for the v1/quote endpoint
#[serde(rename_all = "camelCase")]
#[derive(Debug, Deserialize, Serialize)]
pub struct SwapQuoteResponse {
    /// Quote id
    pub id: Uuid,
    /// Quote creation timestamp in milliseconds
    pub created_at: u128,
    /// Quote expire timestamp in milliseconds
    pub expires_at: u128,
    /// Input coin
    pub input_coin: Coin,
    /// Input address (Generated on the server)
    pub input_address: String,
    /// Input return address (User specified)
    pub input_return_address: Option<String>,
    /// The effective price (Input amount / Output amount)
    pub effective_price: f64,
    /// Output coin
    pub output_coin: Coin,
    /// Output address
    pub output_address: String,
    /// Slippage limit
    pub slippage_limit: f32,
}

/// Request a swap quote
pub async fn swap<T: TransactionProvider>(
    params: SwapQuoteParams,
    provider: Arc<RwLock<T>>,
    config: Config,
) -> Result<SwapQuoteResponse, ResponseError> {
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
            match generate_eth_address(&config.eth_master_root_key, index) {
                Ok(address) => address,
                Err(err) => {
                    warn!("Failed to generate ethereum address: {}", err);
                    return Err(internal_server_error());
                }
            }
        }
        Coin::LOKI => {
            let payment_id = match LokiPaymentId::from_str(&params.input_address_id) {
                Ok(id) => id,
                Err(_) => return Err(bad_request("Incorrect input address id")),
            };

            match payment_id.get_integrated_address(&config.loki_wallet_address) {
                Ok(address) => address.address,
                Err(err) => {
                    warn!("Failed to generate loki address: {}", err);
                    return Err(internal_server_error());
                }
            }
        }
        Coin::BTC => {
            let index = match params.input_address_id.parse::<u64>() {
                Ok(index) => index,
                Err(_) => return Err(bad_request("Incorrect input address id")),
            };
            match generate_btc_address(
                &config.btc_master_root_key,
                index,
                true,
                bitcoin::AddressType::P2wpkh,
                &config.net_type,
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

    Ok(SwapQuoteResponse {
        id: quote.id,
        created_at: quote.timestamp.0,
        expires_at: get_swap_expire_timestamp(&quote.timestamp).0,
        input_coin,
        input_address: input_address.to_string(),
        input_return_address: params.input_return_address,
        effective_price,
        output_coin,
        output_address: params.output_address,
        slippage_limit: params.slippage_limit,
    })
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        common::coins::PoolCoin, transactions::PoolChangeTx,
        utils::test_utils::get_transactions_provider, utils::test_utils::TEST_ROOT_KEY,
        vault::config::NetType,
    };

    fn config() -> Config {
        Config {
            loki_wallet_address: "T6SMsepawgrKXeFmQroAbuTQMqLWyMxiVUgZ6APCRFgxQAUQ1AkEtHxAgDMZJJG9HMJeTeDsqWiuCMsNahScC7ZS2StC9kHhY".to_string(),
            eth_master_root_key: TEST_ROOT_KEY.to_string(),
            btc_master_root_key: TEST_ROOT_KEY.to_string(),
            net_type: NetType::Testnet
        }
    }

    fn params() -> SwapQuoteParams {
        SwapQuoteParams {
            input_coin: Coin::LOKI,
            input_return_address: Some("T6SMsepawgrKXeFmQroAbuTQMqLWyMxiVUgZ6APCRFgxQAUQ1AkEtHxAgDMZJJG9HMJeTeDsqWiuCMsNahScC7ZS2StC9kHhY".to_string()),
            input_address_id: "60900e5603bf96e3".to_owned(),
            input_amount: "1000000000".to_string(),
            output_coin: Coin::ETH,
            output_address: "0x70e7db0678460c5e53f1ffc9221d1c692111dcc5".to_string(),
            slippage_limit: 0.0,
        }
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
            input_address_id: quote_params.input_address_id.clone(),
            return_address: quote_params.input_return_address.clone().map(WalletAddress),
            output: quote_params.output_coin,
            slippage_limit: quote_params.slippage_limit,
            output_address: WalletAddress::new(&quote_params.output_address),
            effective_price: 1.0
        };
        provider.add_transactions(vec![quote.into()]).unwrap();

        let provider = Arc::new(RwLock::new(provider));

        let result = swap(quote_params, provider, config())
            .await
            .expect_err("Expected swap to return error");

        assert_eq!(&result.message, "Quote already exists for input address id");
    }

    #[tokio::test]
    async fn returns_error_if_no_liquidity() {
        let provider = get_transactions_provider();
        let provider = Arc::new(RwLock::new(provider));

        // No pools yet
        let result = swap(params(), provider.clone(), config())
            .await
            .expect_err("Expected swap to return error");

        assert_eq!(&result.message, "Not enough liquidity");

        // Pool with no liquidity
        {
            let tx = PoolChangeTx::new(PoolCoin::ETH, 0, 0);

            let mut provider = provider.write();
            provider.add_transactions(vec![tx.into()]).unwrap();
        }

        let result = swap(params(), provider.clone(), config())
            .await
            .expect_err("Expected swap to return error");

        assert_eq!(&result.message, "Not enough liquidity");
    }

    #[tokio::test]
    async fn returns_response_if_successful() {
        let mut provider = get_transactions_provider();
        let tx = PoolChangeTx::new(PoolCoin::ETH, 10_000_000_000, 50_000_000_000);
        provider.add_transactions(vec![tx.into()]).unwrap();

        let provider = Arc::new(RwLock::new(provider));

        swap(params(), provider.clone(), config())
            .await
            .expect("Expected to get a swap response");

        assert_eq!(provider.read().get_quote_txs().len(), 1);
    }
}
