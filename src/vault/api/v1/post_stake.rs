use crate::{
    common::{api::ResponseError, *},
    transactions::StakeQuoteTx,
    utils::validation::validate_address_id,
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

/// Params for the v1/quote endpoint
#[serde(rename_all = "camelCase")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StakeQuoteParams {
    /// The pool to stake into
    pub pool: Coin,
    /// The staker id
    pub staker_id: String,
    /// The input address if of the other coin
    pub coin_input_address_id: String,
    /// The loki input address id
    pub loki_input_address_id: String,
}

/// Response for the v1/quote endpoint
#[serde(rename_all = "camelCase")]
#[derive(Debug, Deserialize, Serialize)]
pub struct StakeQuoteResponse {
    /// Quote id
    pub id: Uuid,
    /// Quote creation timestamp in milliseconds
    pub created_at: u128,
    /// Quote expire timestamp in milliseconds
    pub expires_at: u128,
    /// Pool coin
    pub pool: Coin,
    /// Staker id
    pub staker_id: String,
    /// Loki input address
    pub loki_input_address: String,
    /// Other coin input address
    pub coin_input_address: String,
}

/// Request a stake quote
pub async fn stake<T: TransactionProvider>(
    params: StakeQuoteParams,
    provider: Arc<RwLock<T>>,
    config: Config,
) -> Result<StakeQuoteResponse, ResponseError> {
    let original_params = params.clone();

    let pool_coin =
        PoolCoin::from(params.pool).map_err(|_| bad_request("Invalid pool specified"))?;

    if let Err(_) = validate_address_id(params.pool, &params.coin_input_address_id) {
        return Err(bad_request("Invalid coin input address id"));
    }

    let loki_input_address_id = LokiPaymentId::from_str(&params.loki_input_address_id)
        .map_err(|_| bad_request("Invalid loki input address id"))?;

    let mut provider = provider.write();
    provider.sync();

    // Ensure we don't have a quote with the input address
    if let Some(_) = provider.get_quote_txs().iter().find(|quote_info| {
        let quote = &quote_info.inner;
        let is_loki_quote =
            quote.input == Coin::LOKI && quote.input_address_id == params.loki_input_address_id;
        let is_other_quote =
            quote.input == params.pool && quote.input_address_id == params.coin_input_address_id;
        is_loki_quote || is_other_quote
    }) {
        return Err(bad_request("Quote already exists for input address id"));
    }

    if let Some(_) = provider.get_stake_quote_txs().iter().find(|quote_info| {
        let quote = &quote_info.inner;
        quote.loki_input_address_id == loki_input_address_id
            || quote.coin_input_address_id == params.coin_input_address_id
    }) {
        return Err(bad_request("Quote already exists for input address id"));
    }

    // Generate addresses
    let coin_input_address = match params.pool {
        Coin::ETH => {
            let index = match params.coin_input_address_id.parse::<u64>() {
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
        Coin::BTC => {
            let index = match params.coin_input_address_id.parse::<u64>() {
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
                params.pool
            );
            return Err(internal_server_error());
        }
    };

    let loki_input_address =
        match loki_input_address_id.get_integrated_address(&config.loki_wallet_address) {
            Ok(address) => address.address,
            Err(err) => {
                warn!("Failed to generate loki address: {}", err);
                return Err(internal_server_error());
            }
        };

    let quote = StakeQuoteTx::new(
        Timestamp::now(),
        pool_coin,
        WalletAddress::new(&coin_input_address),
        params.coin_input_address_id,
        WalletAddress::new(&loki_input_address),
        loki_input_address_id,
        StakerId(params.staker_id.clone()),
    )
    .map_err(|err| {
        error!(
            "Failed to create stakke quote tx for params: {:?} due to error {}",
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

    Ok(StakeQuoteResponse {
        id: quote.id,
        created_at: quote.timestamp.0,
        expires_at: get_swap_expire_timestamp(&quote.timestamp).0,
        staker_id: params.staker_id,
        pool: params.pool,
        loki_input_address,
        coin_input_address,
    })
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        utils::test_utils::create_fake_quote_tx, utils::test_utils::create_fake_stake_quote,
        utils::test_utils::get_transactions_provider, utils::test_utils::TEST_ETH_ADDRESS,
        utils::test_utils::TEST_LOKI_ADDRESS, utils::test_utils::TEST_ROOT_KEY,
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

    fn params() -> StakeQuoteParams {
        StakeQuoteParams {
            pool: Coin::ETH,
            staker_id: "id".to_string(),
            coin_input_address_id: "99999".to_string(),
            loki_input_address_id: "b2d6a87ec06934ff".to_string(),
        }
    }

    #[tokio::test]
    async fn returns_error_if_invalid_pool_coin() {
        let mut quote_params = params();
        quote_params.pool = Coin::LOKI;

        let provider = Arc::new(RwLock::new(get_transactions_provider()));
        let result = stake(quote_params, provider, config())
            .await
            .expect_err("Expected stake to return error");

        assert_eq!(&result.message, "Invalid pool specified");
    }

    #[tokio::test]
    async fn returns_error_if_invalid_coin_input_address_id() {
        let provider = Arc::new(RwLock::new(get_transactions_provider()));

        for coin in vec![Coin::ETH, Coin::BTC] {
            let mut quote_params = params();
            quote_params.pool = coin;
            quote_params.coin_input_address_id = "invalid".to_string();

            let result = stake(quote_params, provider.clone(), config())
                .await
                .expect_err("Expected stake to return error");

            assert_eq!(&result.message, "Invalid coin input address id");
        }
    }

    #[tokio::test]
    async fn returns_error_if_invalid_loki_input_address_id() {
        let provider = Arc::new(RwLock::new(get_transactions_provider()));

        let mut quote_params = params();
        quote_params.pool = Coin::ETH;
        quote_params.loki_input_address_id = "invalid".to_string();

        let result = stake(quote_params, provider.clone(), config())
            .await
            .expect_err("Expected stake to return error");

        assert_eq!(&result.message, "Invalid loki input address id");
    }

    #[tokio::test]
    async fn returns_error_if_swap_quote_with_same_input_address_exists() {
        let quote_params = params();

        let mut loki_quote = create_fake_quote_tx(
            Coin::LOKI,
            WalletAddress::new(TEST_LOKI_ADDRESS),
            Coin::ETH,
            WalletAddress::new(TEST_ETH_ADDRESS),
        );
        loki_quote.input_address_id = quote_params.loki_input_address_id.clone();

        let mut other_quote = create_fake_quote_tx(
            Coin::ETH,
            WalletAddress::new(TEST_ETH_ADDRESS),
            Coin::LOKI,
            WalletAddress::new(TEST_LOKI_ADDRESS),
        );
        other_quote.input_address_id = quote_params.coin_input_address_id.clone();

        // Make sure we're testing the right logic
        assert_eq!(other_quote.input, quote_params.pool);

        for quote in vec![loki_quote, other_quote] {
            let mut provider = get_transactions_provider();
            provider.add_transactions(vec![quote.into()]).unwrap();

            let provider = Arc::new(RwLock::new(provider));

            let result = stake(quote_params.clone(), provider, config())
                .await
                .expect_err("Expected stake to return error");

            assert_eq!(&result.message, "Quote already exists for input address id");
        }
    }

    #[tokio::test]
    async fn returns_error_if_stake_quote_with_same_input_address_exists() {
        let quote_params = params();

        let mut quote_1 = create_fake_stake_quote(PoolCoin::ETH);
        quote_1.loki_input_address_id =
            LokiPaymentId::from_str(&quote_params.loki_input_address_id).unwrap();

        let mut quote_2 = create_fake_stake_quote(PoolCoin::ETH);
        quote_2.coin_input_address_id = quote_params.coin_input_address_id.clone();

        for quote in vec![quote_1, quote_2] {
            let mut provider = get_transactions_provider();
            provider.add_transactions(vec![quote.into()]).unwrap();

            let provider = Arc::new(RwLock::new(provider));

            let result = stake(quote_params.clone(), provider, config())
                .await
                .expect_err("Expected stake to return error");

            assert_eq!(&result.message, "Quote already exists for input address id");
        }
    }

    #[tokio::test]
    async fn returns_response_if_successful() {
        let provider = Arc::new(RwLock::new(get_transactions_provider()));

        stake(params(), provider.clone(), config())
            .await
            .expect("Expected to get a stake response");

        assert_eq!(provider.read().get_stake_quote_txs().len(), 1);
    }
}
