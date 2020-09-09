use crate::common::{ethereum, Coin, LokiWalletAddress};
use std::str::FromStr;

/// Validate an address from the given `coin`
pub fn validate_address(coin: Coin, address: &str) -> Result<(), String> {
    match coin {
        Coin::LOKI => LokiWalletAddress::from_str(address).map(|_| ()),
        Coin::ETH => ethereum::Address::from_str(address)
            .map(|_| ())
            .map_err(|str| str.to_owned()),
        x @ _ => {
            warn!("Address validation missing for {}", x);
            Err("No address validation found".to_owned())
        }
    }
}
