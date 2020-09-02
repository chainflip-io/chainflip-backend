use crate::{
    common::{
        api::ResponseError, ethereum, Coin, LokiPaymentId, LokiWalletAddress, Timestamp,
        WalletAddress,
    },
    side_chain::SideChainTx,
    transactions::QuoteTx,
    utils::price::get_output_amount,
    vault::transactions::TransactionProvider,
};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::{
    str::FromStr,
    sync::{Arc, Mutex},
    time::UNIX_EPOCH,
};
use uuid::Uuid;

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

/// Validate quote params
pub fn validate_quote_params(params: &QuoteParams) -> Result<(), &'static str> {
    // Coins
    if !params.input_coin.is_supported() {
        return Err("Input coin is not supported");
    } else if !params.output_coin.is_supported() {
        return Err("Output coin is not supported");
    }

    if params.input_coin == params.output_coin {
        return Err("Cannot swap between the same coins");
    }

    // Amount

    let input_amount = params.input_amount.parse::<i128>().unwrap_or(0);
    if input_amount <= 0 {
        return Err("Invalid input amount provided");
    }

    // Addresses

    if params.input_coin.get_info().requires_return_address && params.input_return_address.is_none()
    {
        return Err("Input return address not provided");
    }

    let output_address = match params.output_coin {
        Coin::LOKI => LokiWalletAddress::from_str(&params.output_address)
            .map(|_| ())
            .map_err(|_| ()),
        Coin::ETH => ethereum::Address::from_str(&params.output_address)
            .map(|_| ())
            .map_err(|_| ()),
        x => {
            warn!("Failed to handle output address of {}", x);
            Err(())
        }
    };

    if output_address.is_err() {
        return Err("Invalid output address");
    }

    let input_address_id = match params.input_coin {
        Coin::BTC | Coin::ETH => params
            .input_address_id
            .parse::<u64>()
            .map(|_| ())
            .map_err(|_| ()),
        Coin::LOKI => LokiPaymentId::from_str(&params.input_address_id)
            .map(|_| ())
            .map_err(|_| ()),
        x => {
            warn!("Failed to handle input address id of {}", x);
            Err(())
        }
    };

    if input_address_id.is_err() {
        return Err("Invalid input id provided");
    }

    // Slippage

    if params.slippage_limit <= 0.0 {
        return Err("Slippage limit must be greater than or equal to 0");
    }

    Ok(())
}

fn bad_request(message: &str) -> ResponseError {
    ResponseError::new(StatusCode::BAD_REQUEST, message)
}

fn internal_server_error() -> ResponseError {
    ResponseError::new(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error")
}

/// Request a swap quote
pub async fn post_quote<T: TransactionProvider>(
    params: QuoteParams,
    provider: Arc<Mutex<T>>,
) -> Result<QuoteResponse, ResponseError> {
    if let Err(err) = validate_quote_params(&params) {
        return Err(bad_request(err));
    }

    // Validation of these should have been handled above
    let input_coin = params.input_coin;
    let output_coin = params.output_coin;
    let input_amount = params
        .input_amount
        .parse::<u128>()
        .map_err(|_| internal_server_error())?;

    let mut provider = provider.lock().unwrap();
    provider.sync();

    // Ensure we don't have a quote with the address
    if let Some(_) = provider.get_quote_txs().iter().find(|quote| {
        quote.input == input_coin && quote.input_address_id == params.input_address_id
    }) {
        return Err(bad_request("Quote already exists for input address id"));
    }

    // Calculate the output amount
    let estimated_output_amount =
        get_output_amount(input_coin, input_amount, output_coin, |pool_coin| {
            provider.get_liquidity(pool_coin)
        })
        .map(|vec| {
            if let Some(value) = vec.last() {
                value.1
            } else {
                0u128
            }
        })
        .unwrap_or(0);
    if estimated_output_amount == 0 {
        return Err(bad_request("Not enough liquidity"));
    }

    // Generate addresses
    let input_address = match input_coin {
        Coin::ETH => {
            // TODO: Derive address from input_address_id
            "0x70e7db0678460c5e53f1ffc9221d1c692111dcc5"
        }
        Coin::LOKI => {
            // TODO: Generate integrated address here
            "T6SMsepawgrKXeFmQroAbuTQMqLWyMxiVUgZ6APCRFgxQAUQ1AkEtHxAgDMZJJG9HMJeTeDsqWiuCMsNahScC7ZS2StC9kHhY"
        }
        _ => {
            warn!(
                "Input address generation not implemented for {}",
                input_coin
            );
            return Err(internal_server_error());
        }
    };

    let quote = QuoteTx {
        id: Uuid::new_v4(),
        timestamp: Timestamp::now(),
        input: input_coin,
        input_amount,
        input_address: WalletAddress::new(input_address),
        input_address_id: params.input_address_id,
        return_address: params.input_return_address.clone().map(WalletAddress),
        output: output_coin,
        slippage_limit: params.slippage_limit,
    };

    provider
        .add_transactions(vec![SideChainTx::QuoteTx(quote.clone())])
        .map_err(|err| {
            error!("Failed to add quote transaction: {}", err);
            internal_server_error()
        })?;

    let created_at = quote
        .timestamp
        .0
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(|_| internal_server_error())?;

    Ok(QuoteResponse {
        id: quote.id,
        created_at,
        // TODO: Implement expiration
        expires_at: 0,
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
