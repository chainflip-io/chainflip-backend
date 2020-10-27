use crate::{
    common::{api::ResponseError, Coin, LokiPaymentId, PoolCoin, Timestamp, WalletAddress},
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

    let pool_coin = match PoolCoin::from(params.pool) {
        Ok(coin) => coin,
        Err(_) => return Err(bad_request("Invalid pool specified")),
    };

    if let Err(_) = validate_address_id(params.pool, &params.coin_input_address_id) {
        return Err(bad_request("Invalid coin input address id"));
    }

    let loki_input_address_id = match LokiPaymentId::from_str(&params.loki_input_address_id) {
        Ok(id) => id,
        Err(_) => return Err(bad_request("Invalid loki input address id")),
    };

    let mut provider = provider.write();
    provider.sync();

    // Ensure we don't have a quote with the address
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
                false,
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
        params.staker_id.clone(),
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

    // TODO: Add tests
}
