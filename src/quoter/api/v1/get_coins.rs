use crate::common::{
    api::ResponseError,
    coins::{Coin, CoinInfo},
};
use serde::Deserialize;

/// Parameters for `coins` endpoint
#[derive(Debug, Deserialize)]
pub struct CoinsParams {
    /// The list of coin symbols
    pub symbols: Option<Vec<String>>,
}

/// Get coins that we support.
///
/// If `symbols` is empty then all coins will be returned.
/// If `symbols` is not empty then only information for valid symbols will be returned.
///
/// # Example Query
///
/// > GET /v1/coins?symbols=BTC,loki
pub async fn get_coins(params: CoinsParams) -> Result<Vec<CoinInfo>, ResponseError> {
    // Return all coins if no params were passed
    if params.symbols.is_none() {
        return Ok(Coin::SUPPORTED.iter().map(|coin| coin.get_info()).collect());
    }

    // Filter out invalid symbols
    let valid_symbols: Vec<Coin> = params
        .symbols
        .unwrap()
        .iter()
        .filter_map(|symbol| symbol.parse::<Coin>().ok())
        .collect();

    let info = valid_symbols.iter().map(|coin| coin.get_info()).collect();

    return Ok(info);
}
