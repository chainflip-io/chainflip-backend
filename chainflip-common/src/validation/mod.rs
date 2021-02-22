use crate::types::{
    addresses::{BitcoinAddress, EthereumAddress, OxenAddress},
    coin::Coin,
    Network,
};
use std::{convert::TryInto, str::FromStr};

/// Validate an address from the given `coin`
pub fn validate_address(coin: Coin, network: Network, address: &str) -> Result<(), &'static str> {
    match coin {
        Coin::OXEN => {
            let address = OxenAddress::from_str(address)?;
            if address.network() != network {
                return Err("Invalid network type");
            };

            Ok(())
        }
        Coin::ETH => EthereumAddress::from_str(address).map(|_| ()),
        Coin::BTC => {
            let address = BitcoinAddress::from_str(address)?;
            if address.network != network {
                return Err("Invalid network type");
            }

            Ok(())
        }
    }
}

/// Validate an address id for the given coin
pub fn validate_address_id(coin: Coin, address_id: &[u8]) -> Result<(), &'static str> {
    match coin {
        Coin::ETH => {
            let _: [u8; 32] = address_id
                .try_into()
                .map_err(|_| "Invalid ethereum address salt")?;
            Ok(())
        }
        Coin::BTC => {
            let bytes: [u8; 4] = address_id
                .try_into()
                .map_err(|_| "Address id must be a u32 integer")?;
            let index = u32::from_be_bytes(bytes);
            // Index 0 is used for the main wallet and 1-4 are reserved for future use
            if index < 5 {
                Err("Address id must be greater than 5")
            } else {
                Ok(())
            }
        }
        Coin::OXEN => {
            let _: [u8; 8] = address_id
                .try_into()
                .map_err(|_| "Invalid oxen payment id")?;
            Ok(())
        }
    }
}

/// Validate a staker id
pub fn validate_staker_id(id: &[u8]) -> Result<(), &'static str> {
    const PUBKEY_LEN: usize = 65;

    if id.len() == PUBKEY_LEN {
        Ok(())
    } else {
        Err("Unexpected staker id length")
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test::constants::{
        BTC_ADDRESS, ETH_ADDRESS, OXEN_ADDRESS, OXEN_PAYMENT_ID, STAKER_ID,
    };

    #[test]
    pub fn validates_address() {
        let invalid = "hello";

        // oxen
        assert!(validate_address(Coin::OXEN, Network::Mainnet, OXEN_ADDRESS).is_ok());
        assert!(validate_address(Coin::OXEN, Network::Mainnet, invalid).is_err());
        assert!(validate_address(Coin::OXEN, Network::Mainnet, "").is_err());

        let oxen_testnet_address = "T6SiGp1EuAB5qE8jqdMu7pHr8Tw5DcQtKT34MxjxY5tjUyzZ3QSHcsS78fVw4B2iKqgqnfB2H1Sac5BG1yWD7NLq2Q41A7EqV";
        assert!(validate_address(Coin::OXEN, Network::Testnet, oxen_testnet_address).is_ok());
        assert!(validate_address(Coin::OXEN, Network::Mainnet, oxen_testnet_address).is_err());

        // eth
        assert!(validate_address(Coin::ETH, Network::Mainnet, ETH_ADDRESS).is_ok());
        assert!(validate_address(Coin::ETH, Network::Testnet, ETH_ADDRESS).is_ok());
        assert!(validate_address(Coin::ETH, Network::Mainnet, invalid).is_err());
        assert!(validate_address(Coin::ETH, Network::Mainnet, "").is_err());

        // btc
        assert!(&validate_address(Coin::BTC, Network::Mainnet, BTC_ADDRESS).is_ok());
        assert!(validate_address(Coin::BTC, Network::Mainnet, "").is_err());

        let btc_testnet_address = "tb1q6898gg3tkkjurdpl4cghaqgmyvs29p4x4h0552";
        assert!(&validate_address(Coin::BTC, Network::Testnet, btc_testnet_address).is_ok());
        assert!(&validate_address(Coin::BTC, Network::Mainnet, btc_testnet_address).is_err());
        assert!(&validate_address(Coin::BTC, Network::Mainnet, invalid).is_err());
    }

    #[test]
    pub fn validates_eth_address_id() {
        let bytes = vec![0; 32];
        assert!(validate_address_id(Coin::ETH, &bytes).is_ok());
        assert!(validate_address_id(Coin::ETH, b"5").is_err());
        assert!(validate_address_id(Coin::ETH, b"invalid").is_err());
    }

    #[test]
    pub fn validates_btc_address_id() {
        assert!(validate_address_id(Coin::BTC, &5u32.to_be_bytes()).is_ok());
        assert_eq!(
            validate_address_id(Coin::BTC, &4u32.to_be_bytes()).unwrap_err(),
            "Address id must be greater than 5"
        );
        assert_eq!(
            validate_address_id(Coin::BTC, b"id").unwrap_err(),
            "Address id must be a u32 integer"
        );

        assert_eq!(
            validate_address_id(Coin::BTC, &(-5i64).to_be_bytes()).unwrap_err(),
            "Address id must be a u32 integer"
        );
    }

    #[test]
    pub fn validates_oxen_address_id() {
        assert!(validate_address_id(Coin::OXEN, &OXEN_PAYMENT_ID.bytes()).is_ok());
        assert!(validate_address_id(Coin::OXEN, b"5").is_err());
        assert!(validate_address_id(Coin::OXEN, b"invalid").is_err());
    }

    #[test]
    pub fn validates_staker_id() {
        assert!(validate_staker_id(&STAKER_ID).is_ok());
        assert_eq!(
            validate_staker_id(&[]).unwrap_err(),
            "Unexpected staker id length"
        );
    }
}
