use super::{
    utils::{bad_request, internal_server_error},
    Config,
};
use crate::{
    common::{api::ResponseError, *},
    utils::address::generate_btc_address_from_index,
    vault::{processor::utils::get_swap_expire_timestamp, transactions::TransactionProvider},
};
use chainflip_common::{
    constants::ethereum,
    types::{
        addresses::{EthereumAddress, OxenAddress},
        chain::{DepositQuote, Validate},
        coin::Coin,
        Timestamp,
    },
    utils::address_id,
    validation::{validate_address, validate_address_id},
};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::{convert::TryInto, str::FromStr, sync::Arc};

/// Params for the v1/quote endpoint
#[serde(rename_all = "camelCase")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositQuoteParams {
    /// The pool to deposit into
    pub pool: Coin,
    /// The staker id
    pub staker_id: String,
    /// The input address if of the other coin
    pub coin_input_address_id: String,
    /// The oxen input address id
    pub oxen_input_address_id: String,
    /// Address to return Oxen to if deposit quote already fulfilled
    pub oxen_return_address: String,
    /// Address to return other coin to if deposit quote already fulfilled
    pub other_return_address: String,
}

/// Response for the v1/quote endpoint
#[serde(rename_all = "camelCase")]
#[derive(Debug, Deserialize, Serialize)]
pub struct DepositQuoteResponse {
    /// Quote creation timestamp in milliseconds
    pub created_at: u128,
    /// Quote expire timestamp in milliseconds
    pub expires_at: u128,
    /// Pool coin
    pub pool: Coin,
    /// Staker id
    pub staker_id: String,
    /// Oxen input address
    pub oxen_input_address: String,
    /// Oxen return address
    pub oxen_return_address: String,
    /// Other coin input address
    pub coin_input_address: String,
    /// Other coin return address
    pub coin_return_address: String,
}

/// Request a deposit quote
pub async fn deposit<T: TransactionProvider>(
    params: DepositQuoteParams,
    provider: Arc<RwLock<T>>,
    config: Config,
) -> Result<DepositQuoteResponse, ResponseError> {
    let original_params = params.clone();

    let pool_coin =
        PoolCoin::from(params.pool).map_err(|_| bad_request("Invalid pool specified"))?;

    let base_input_address_id =
        address_id::to_bytes(Coin::BASE_COIN, &params.oxen_input_address_id)
            .map_err(|_| bad_request("Invalid base input address id"))?;

    if let Err(_) = validate_address_id(Coin::BASE_COIN, &base_input_address_id) {
        return Err(bad_request("Invalid base input address id"));
    }

    let coin_input_address_id = address_id::to_bytes(params.pool, &params.coin_input_address_id)
        .map_err(|_| bad_request("Invalid coin input address id"))?;

    if let Err(_) = validate_address_id(params.pool, &coin_input_address_id) {
        return Err(bad_request("Invalid coin input address id"));
    }

    if let Err(_) = validate_address(Coin::OXEN, config.net_type, &params.oxen_return_address) {
        return Err(bad_request("Invalid oxen return address"));
    }

    if let Err(_) = validate_address(params.pool, config.net_type, &params.other_return_address) {
        return Err(bad_request("Invalid other return address"));
    }

    let mut provider = provider.write();
    provider.sync();

    // Ensure we don't have a quote with the input address
    if let Some(_) = provider.get_swap_quotes().iter().find(|quote_info| {
        let quote = &quote_info.inner;
        let is_base_quote =
            quote.input == Coin::BASE_COIN && quote.input_address_id == base_input_address_id;
        let is_other_quote =
            quote.input == params.pool && quote.input_address_id == coin_input_address_id;
        is_base_quote || is_other_quote
    }) {
        return Err(bad_request("Quote already exists for input address id"));
    }

    if let Some(_) = provider.get_deposit_quotes().iter().find(|quote_info| {
        let quote = &quote_info.inner;
        quote.base_input_address_id == base_input_address_id
            || quote.coin_input_address_id == coin_input_address_id
    }) {
        return Err(bad_request("Quote already exists for input address id"));
    }

    // Generate addresses
    let coin_input_address = match params.pool {
        Coin::ETH => {
            let vault_address = ethereum::get_vault_address(config.net_type);

            let salt = coin_input_address_id.clone().try_into().map_err(|_| {
                warn!("Failed to convert coin input address id to ethereum salt");
                internal_server_error()
            })?;

            EthereumAddress::create2(&vault_address, salt, &ethereum::ETH_DEPOSIT_INIT_CODE)
                .to_string()
        }
        Coin::BTC => {
            let index = match params.coin_input_address_id.parse::<u32>() {
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
        _ => {
            warn!(
                "Input address generation not implemented for {}",
                params.pool
            );
            return Err(internal_server_error());
        }
    };

    let oxen_base_address = OxenAddress::from_str(&config.oxen_wallet_address)
        .expect("Expected valid oxen wallet address");

    let payment_id = base_input_address_id.clone().try_into().map_err(|_| {
        warn!("Failed to convert base input address id to oxen payment id");
        internal_server_error()
    })?;
    let base_input_address = oxen_base_address.with_payment_id(Some(payment_id));
    assert_eq!(base_input_address.network(), config.net_type);

    let staker_id =
        StakerId::new(params.staker_id.clone()).map_err(|_| bad_request("Invalid staker id"))?;

    let quote = DepositQuote {
        timestamp: Timestamp::now(),
        staker_id: staker_id.bytes().to_vec(),
        pool: pool_coin.get_coin(),
        coin_input_address: coin_input_address.clone().into(),
        coin_input_address_id,
        coin_return_address: params.other_return_address.into(),
        base_input_address: base_input_address.to_string().into(),
        base_input_address_id,
        base_return_address: params.oxen_return_address.into(),
        event_number: None,
    };

    quote.validate(config.net_type).map_err(|err| {
        error!(
            "Failed to create deposit quote for params: {:?} due to error {}",
            original_params,
            err.clone()
        );
        bad_request!("{}", err)
    })?;

    provider
        .add_local_events(vec![quote.clone().into()])
        .map_err(|err| {
            error!("Failed to add deposit quote: {}", err);
            internal_server_error()
        })?;

    Ok(DepositQuoteResponse {
        created_at: quote.timestamp.0,
        expires_at: get_swap_expire_timestamp(&quote.timestamp).0,
        staker_id: params.staker_id,
        pool: params.pool,
        oxen_input_address: base_input_address.to_string(),
        coin_input_address,
        oxen_return_address: quote.base_return_address.to_string(),
        coin_return_address: quote.coin_return_address.to_string(),
    })
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::utils::test_utils::{
        self, get_transactions_provider, staking::get_random_staker, TEST_ETH_ADDRESS,
        TEST_ETH_SALT, TEST_OXEN_ADDRESS, TEST_ROOT_KEY,
    };
    use chainflip_common::types::Network;
    use test_utils::data::TestData;

    fn config() -> Config {
        Config {
            oxen_wallet_address: "T6SMsepawgrKXeFmQroAbuTQMqLWyMxiVUgZ6APCRFgxQAUQ1AkEtHxAgDMZJJG9HMJeTeDsqWiuCMsNahScC7ZS2StC9kHhY".to_string(),
            btc_master_root_key: TEST_ROOT_KEY.to_string(),
            net_type: Network::Testnet
        }
    }

    fn params() -> DepositQuoteParams {
        DepositQuoteParams {
            pool: Coin::ETH,
            staker_id: get_random_staker().public_key(),
            coin_input_address_id: hex::encode(TEST_ETH_SALT),
            oxen_input_address_id: "b2d6a87ec06934ff".to_string(),
            oxen_return_address: TEST_OXEN_ADDRESS.to_string(),
            other_return_address: TEST_ETH_ADDRESS.to_string(),
        }
    }

    #[tokio::test]
    async fn returns_error_if_invalid_pool_coin() {
        let mut quote_params = params();
        quote_params.pool = Coin::OXEN;

        let provider = Arc::new(RwLock::new(get_transactions_provider()));
        let result = deposit(quote_params, provider, config())
            .await
            .expect_err("Expected deposit to return error");

        assert_eq!(&result.message, "Invalid pool specified");
    }

    #[tokio::test]
    async fn returns_error_if_invalid_coin_input_address_id() {
        let provider = Arc::new(RwLock::new(get_transactions_provider()));

        for coin in vec![Coin::ETH, Coin::BTC] {
            let mut quote_params = params();
            quote_params.pool = coin;
            quote_params.coin_input_address_id = "invalid".to_string();

            let result = deposit(quote_params, provider.clone(), config())
                .await
                .expect_err("Expected deposit to return error");

            assert_eq!(&result.message, "Invalid coin input address id");
        }
    }

    #[tokio::test]
    async fn returns_error_if_invalid_oxen_input_address_id() {
        let provider = Arc::new(RwLock::new(get_transactions_provider()));

        let mut quote_params = params();
        quote_params.pool = Coin::ETH;
        quote_params.oxen_input_address_id = "invalid".to_string();

        let result = deposit(quote_params, provider.clone(), config())
            .await
            .expect_err("Expected deposit to return error");

        assert_eq!(&result.message, "Invalid base input address id");
    }

    #[tokio::test]
    async fn returns_error_if_invalid_oxen_return_address() {
        let provider = Arc::new(RwLock::new(get_transactions_provider()));

        let mut quote_params = params();
        quote_params.oxen_return_address = "invalid".to_string();

        let result = deposit(quote_params, provider.clone(), config())
            .await
            .expect_err("Expected deposit to return error");

        assert_eq!(&result.message, "Invalid oxen return address");
    }

    #[tokio::test]
    async fn returns_error_if_invalid_other_return_address() {
        let provider = Arc::new(RwLock::new(get_transactions_provider()));

        let mut quote_params = params();
        quote_params.other_return_address = "invalid".to_string();

        let result = deposit(quote_params, provider.clone(), config())
            .await
            .expect_err("Expected deposit to return error");

        assert_eq!(&result.message, "Invalid other return address");
    }

    #[tokio::test]
    async fn returns_error_if_swap_quote_with_same_input_address_exists() {
        let quote_params = params();

        let mut oxen_quote = TestData::swap_quote(Coin::OXEN, Coin::ETH);
        oxen_quote.input_address_id =
            address_id::to_bytes(Coin::OXEN, &quote_params.oxen_input_address_id).unwrap();

        let mut other_quote = TestData::swap_quote(Coin::ETH, Coin::OXEN);
        other_quote.input_address_id =
            address_id::to_bytes(Coin::ETH, &quote_params.coin_input_address_id).unwrap();

        // Make sure we're testing the right logic
        assert_eq!(other_quote.input, quote_params.pool);

        for quote in vec![oxen_quote, other_quote] {
            let mut provider = get_transactions_provider();
            provider.add_local_events(vec![quote.into()]).unwrap();

            let provider = Arc::new(RwLock::new(provider));

            let result = deposit(quote_params.clone(), provider, config())
                .await
                .expect_err("Expected deposit to return error");

            assert_eq!(&result.message, "Quote already exists for input address id");
        }
    }

    #[tokio::test]
    async fn returns_error_if_deposit_quote_with_same_input_address_exists() {
        let quote_params = params();

        let mut quote_1 = TestData::deposit_quote(Coin::ETH);
        quote_1.base_input_address_id =
            address_id::to_bytes(Coin::OXEN, &quote_params.oxen_input_address_id).unwrap();

        let mut quote_2 = TestData::deposit_quote(Coin::ETH);
        quote_2.coin_input_address_id =
            address_id::to_bytes(Coin::ETH, &quote_params.coin_input_address_id).unwrap();

        for quote in vec![quote_1, quote_2] {
            let mut provider = get_transactions_provider();
            provider.add_local_events(vec![quote.into()]).unwrap();

            let provider = Arc::new(RwLock::new(provider));

            let result = deposit(quote_params.clone(), provider, config())
                .await
                .expect_err("Expected deposit to return error");

            assert_eq!(&result.message, "Quote already exists for input address id");
        }
    }

    #[tokio::test]
    async fn returns_response_if_successful() {
        let provider = Arc::new(RwLock::new(get_transactions_provider()));

        deposit(params(), provider.clone(), config())
            .await
            .expect("Expected to get a deposit response");

        assert_eq!(provider.read().get_deposit_quotes().len(), 1);
    }
}
