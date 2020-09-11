use crate::common::{ethereum, Coin, LokiPaymentId, LokiWalletAddress};
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

/// Validate an address id for the given coin
pub fn validate_address_id(coin: Coin, address_id: &str) -> Result<(), String> {
    match coin {
        Coin::BTC | Coin::ETH => match address_id.parse::<u64>() {
            // Index 0 is used for the main wallet and 1-4 are reserved for future use
            Ok(id) => {
                if id < 5 {
                    Err("Address id must be greater than 5".to_owned())
                } else {
                    Ok(())
                }
            }
            Err(_) => Err("Address id must be an signed integer".to_owned()),
        },
        Coin::LOKI => LokiPaymentId::from_str(address_id).map(|_| ()),
        x @ _ => {
            warn!("Address id validation missing for {}", x);
            Err("No address if validation found".to_owned())
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    pub fn validates_address() {
        let invalid = "hello";
        let loki_address = "T6SMsepawgrKXeFmQroAbuTQMqLWyMxiVUgZ6APCRFgxQAUQ1AkEtHxAgDMZJJG9HMJeTeDsqWiuCMsNahScC7ZS2StC9kHhY";
        let eth_address = "0x70e7db0678460c5e53f1ffc9221d1c692111dcc5";

        assert!(validate_address(Coin::LOKI, loki_address).is_ok());
        assert!(validate_address(Coin::LOKI, invalid).is_err());

        assert!(validate_address(Coin::ETH, eth_address).is_ok());
        assert!(validate_address(Coin::ETH, invalid).is_err());
    }

    #[test]
    pub fn validates_eth_address_id() {
        assert!(validate_address_id(Coin::ETH, "5").is_ok());
        assert_eq!(
            &validate_address_id(Coin::ETH, "4").unwrap_err(),
            "Address id must be greater than 5"
        );
        assert_eq!(
            validate_address_id(Coin::ETH, "id").unwrap_err(),
            "Address id must be an signed integer"
        );
        assert_eq!(
            validate_address_id(Coin::ETH, "-5").unwrap_err(),
            "Address id must be an signed integer"
        );
    }

    #[test]
    pub fn validates_btc_address_id() {
        assert!(validate_address_id(Coin::BTC, "5").is_ok());
        assert_eq!(
            &validate_address_id(Coin::BTC, "4").unwrap_err(),
            "Address id must be greater than 5"
        );
        assert_eq!(
            validate_address_id(Coin::BTC, "id").unwrap_err(),
            "Address id must be an signed integer"
        );
        assert_eq!(
            validate_address_id(Coin::BTC, "-5").unwrap_err(),
            "Address id must be an signed integer"
        );
    }

    #[test]
    pub fn validates_loki_address_id() {
        assert!(validate_address_id(Coin::LOKI, "60900e5603bf96e3").is_ok());
        assert!(validate_address_id(
            Coin::LOKI,
            "60900e5603bf96e3000000000000000000000000000000000000000000000000"
        )
        .is_ok());

        assert!(validate_address_id(Coin::LOKI, "5").is_err());
        assert!(validate_address_id(Coin::LOKI, "invalid").is_err());
        assert!(validate_address_id(Coin::LOKI, "60900e5603bf96H").is_err());
    }
}
