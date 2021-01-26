use super::{
    utils::{bad_request, internal_server_error},
    Config,
};
use crate::{
    common::api::ResponseError,
    utils::{
        address::{generate_btc_address_from_index, generate_eth_address},
        calculate_effective_price, price,
    },
    vault::{processor::utils::get_swap_expire_timestamp, transactions::TransactionProvider},
};
use chainflip_common::{
    constants::ethereum::ETH_DEPOSIT_INIT_CODE,
    types::{
        addresses::{EthereumAddress, LokiAddress},
        chain::{SwapQuote, Validate},
        coin::Coin,
        fraction::PercentageFraction,
        Timestamp, UUIDv4,
    },
    utils::address_id,
};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::{convert::TryInto, str::FromStr, sync::Arc};

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
    pub slippage_limit: u32,
}

/// Response for the v1/quote endpoint
#[serde(rename_all = "camelCase")]
#[derive(Debug, Deserialize, Serialize)]
pub struct SwapQuoteResponse {
    /// Quote id
    pub id: UUIDv4,
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
    /// The effective price ((Input amount << 64) / Output amount)
    pub effective_price: u128,
    /// Output coin
    pub output_coin: Coin,
    /// Output address
    pub output_address: String,
    /// Slippage limit
    pub slippage_limit: u32,
}

/// Request a swap quote
pub async fn swap<T: TransactionProvider>(
    params: SwapQuoteParams,
    provider: Arc<RwLock<T>>,
    config: Config,
) -> Result<SwapQuoteResponse, ResponseError> {
    let original_params = params.clone();

    if let Err(err) = validation::validate_params(&params, config.net_type) {
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

    // Validation of this happens above
    let input_address_id = address_id::to_bytes(input_coin, &params.input_address_id)
        .map_err(|_| bad_request("Invalid input address id"))?;

    // Ensure we don't have a quote with the address
    if let Some(_) = provider.get_swap_quotes().iter().find(|quote_info| {
        let quote = &quote_info.inner;
        quote.input == input_coin && quote.input_address_id == input_address_id
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

    let effective_price = match calculate_effective_price(input_amount, estimated_output_amount) {
        Some(price) => price,
        None => {
            warn!(
                "Failed to calculate effective price for input {} and output {}",
                input_amount, estimated_output_amount
            );
            return Err(internal_server_error());
        }
    };

    // Generate addresses
    let input_address = match input_coin {
        Coin::ETH => {
            // Main vault address
            // Currently we just assume it's at index 0 but we can change it in the future
            let root_address = match generate_eth_address(&config.eth_master_root_key, 0) {
                Ok(address) => address,
                Err(err) => {
                    warn!("Failed to generate ethereum address: {}", err);
                    return Err(internal_server_error());
                }
            };

            let salt = input_address_id.clone().try_into().map_err(|_| {
                warn!("Failed to convert input address id to ethereum salt");
                internal_server_error()
            })?;

            EthereumAddress::create2(&root_address, salt, &ETH_DEPOSIT_INIT_CODE).to_string()
        }
        Coin::LOKI => {
            let loki_base_address = LokiAddress::from_str(&config.loki_wallet_address)
                .expect("Expected valid loki wallet address");
            let payment_id = input_address_id.clone().try_into().map_err(|_| {
                warn!("Failed to convert input address id to loki payment id");
                internal_server_error()
            })?;
            let base_input_address = loki_base_address.with_payment_id(Some(payment_id));
            assert_eq!(base_input_address.network(), config.net_type);

            base_input_address.to_string()
        }
        Coin::BTC => {
            let index = match params.input_address_id.parse::<u32>() {
                Ok(index) => index,
                Err(_) => return Err(bad_request("Incorrect input address id")),
            };
            match generate_btc_address_from_index(
                &config.btc_master_root_key,
                index,
                true,
                bitcoin::AddressType::P2wpkh,
                config.net_type,
            ) {
                Ok(address) => address,
                Err(err) => {
                    warn!("Failed to generate bitcoin address: {}", err);
                    return Err(internal_server_error());
                }
            }
        }
    };

    let slippage_limit = {
        if params.slippage_limit > 0 {
            let fraction =
                PercentageFraction::new(params.slippage_limit).map_err(|err| bad_request(err))?;
            Some(fraction)
        } else {
            None
        }
    };

    let quote = SwapQuote {
        id: UUIDv4::new(),
        timestamp: Timestamp::now(),
        input: input_coin,
        input_address: input_address.clone().into(),
        input_address_id,
        return_address: params.input_return_address.clone().map(|id| id.into()),
        output: output_coin,
        output_address: params.output_address.clone().into(),
        effective_price,
        slippage_limit,
    };

    if let Err(err) = quote.validate(config.net_type) {
        error!(
            "Failed to create swap quote for params: {:?} due to error {}",
            original_params, err
        );
        return Err(bad_request(err));
    }

    provider
        .add_transactions(vec![quote.clone().into()])
        .map_err(|err| {
            error!("Failed to add swap quote: {}", err);
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
    use chainflip_common::types::{chain::PoolChange, Network};

    use super::*;
    use crate::{utils::test_utils::get_transactions_provider, utils::test_utils::TEST_ROOT_KEY};

    fn config() -> Config {
        Config {
            loki_wallet_address: "T6SMsepawgrKXeFmQroAbuTQMqLWyMxiVUgZ6APCRFgxQAUQ1AkEtHxAgDMZJJG9HMJeTeDsqWiuCMsNahScC7ZS2StC9kHhY".to_string(),
            eth_master_root_key: TEST_ROOT_KEY.to_string(),
            btc_master_root_key: TEST_ROOT_KEY.to_string(),
            net_type: Network::Testnet
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
            slippage_limit: 0,
        }
    }

    #[tokio::test]
    async fn returns_error_if_quote_exists() {
        let quote_params = params();

        let mut provider = get_transactions_provider();
        let quote = SwapQuote {
            id: UUIDv4::new(),
            timestamp: Timestamp::now(),
            input: quote_params.input_coin,
            input_address: "T6SMsepawgrKXeFmQroAbuTQMqLWyMxiVUgZ6APCRFgxQAUQ1AkEtHxAgDMZJJG9HMJeTeDsqWiuCMsNahScC7ZS2StC9kHhY".into(),
            input_address_id: address_id::to_bytes(Coin::LOKI, &quote_params.input_address_id).unwrap(),
            return_address: quote_params.input_return_address.clone().map(|id| id.into()),
            output: quote_params.output_coin,
            slippage_limit: None,
            output_address: quote_params.output_address.clone().into(),
            effective_price: 1
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
            let tx = PoolChange {
                id: UUIDv4::new(),
                timestamp: Timestamp::now(),
                pool: Coin::ETH,
                depth_change: 0,
                base_depth_change: 0,
            };

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
        let tx = PoolChange {
            id: UUIDv4::new(),
            timestamp: Timestamp::now(),
            pool: Coin::ETH,
            depth_change: 10_000_000_000,
            base_depth_change: 50_000_000_000,
        };
        provider.add_transactions(vec![tx.into()]).unwrap();

        let provider = Arc::new(RwLock::new(provider));

        swap(params(), provider.clone(), config())
            .await
            .expect("Expected to get a swap response");

        assert_eq!(provider.read().get_swap_quotes().len(), 1);
    }
}
