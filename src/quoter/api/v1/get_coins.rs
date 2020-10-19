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

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    pub async fn returns_all_coins() {
        let params = CoinsParams { symbols: None };
        let result = get_coins(params).await.expect("Expected result to be Ok.");
        assert_eq!(result.len(), Coin::SUPPORTED.len());
    }

    #[tokio::test]
    pub async fn returns_coin_information() {
        let params = CoinsParams {
            symbols: Some(vec![
                "eth".to_owned(),
                "LOKI".to_owned(),
                "invalid_coin".to_owned(),
            ]),
        };
        let result = get_coins(params).await.expect("Expected result to be Ok.");

        assert_eq!(result.len(), 2, "Expected get_coins to return 2 CoinInfo");

        for info in result {
            match info.symbol {
                Coin::ETH | Coin::LOKI => continue,
                coin @ _ => panic!("Result returned unexpected coin: {}", coin),
            }
        }
    }
}
