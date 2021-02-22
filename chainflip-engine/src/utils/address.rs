use crate::utils::bip44::{self, KeyPair};
use chainflip_common::types::{addresses::EthereumAddress, Network};

/// Generate bip44 keypair for a master root key
pub fn generate_bip44_keypair_from_root_key(
    root_key: &str,
    coin: bip44::CoinType,
    index: u32,
) -> Result<bip44::KeyPair, String> {
    let root_key = bip44::RawKey::decode(root_key).map_err(|err| format!("{}", err))?;

    let root_key = root_key
        .to_private_key()
        .ok_or("Failed to generate extended private key".to_owned())?;

    let key_pair = bip44::get_key_pair(root_key, coin, index)?;

    return Ok(key_pair);
}

/// Generate an eth address from a master root key and index
pub fn generate_eth_address(root_key: &str, index: u32) -> Result<EthereumAddress, String> {
    let key_pair =
        generate_bip44_keypair_from_root_key(root_key, bip44::CoinType::ETH, index).unwrap();

    Ok(EthereumAddress::from_public_key(
        key_pair.public_key.serialize_uncompressed(),
    ))
}

/// Generate an btc address from a master root key and index and other params
pub fn generate_btc_address_from_index(
    root_key: &str,
    index: u32,
    compressed: bool,
    address_type: bitcoin::AddressType,
    nettype: Network,
) -> Result<String, String> {
    let key_pair = generate_bip44_keypair_from_root_key(root_key, bip44::CoinType::BTC, index)?;

    generate_btc_address(key_pair, compressed, address_type, nettype)
}

/// Generates a BTC address
pub fn generate_btc_address(
    key_pair: KeyPair,
    compressed: bool,
    address_type: bitcoin::AddressType,
    nettype: Network,
) -> Result<String, String> {
    let btc_pubkey = bitcoin::PublicKey {
        key: bitcoin::secp256k1::PublicKey::from_slice(&key_pair.public_key.serialize()).unwrap(),
        compressed,
    };

    let network = match nettype {
        Network::Testnet => bitcoin::Network::Testnet,
        Network::Mainnet => bitcoin::Network::Bitcoin,
    };

    let address = match address_type {
        // throw error that says must use compressed public key format
        bitcoin::AddressType::P2wpkh => {
            bitcoin::Address::p2wpkh(&btc_pubkey, network).map_err(|e| e.to_string())?
        }
        bitcoin::AddressType::P2pkh => bitcoin::Address::p2pkh(&btc_pubkey, network),
        _ => {
            warn!(
                "Address type of {} is not currently supported. Defaulting to p2wpkh address",
                address_type
            );
            bitcoin::Address::p2wpkh(&btc_pubkey, network).map_err(|e| e.to_string())?
        }
    };
    let address = address.to_string();

    Ok(address)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::utils::test_utils::TEST_ROOT_KEY;
    use bitcoin::AddressType::*;

    #[test]
    fn test_generate_bip44_keypair_from_root_key() {
        for coin in vec![bip44::CoinType::BTC, bip44::CoinType::ETH] {
            for index in vec![0, 500, u32::MAX] {
                assert!(
                    generate_bip44_keypair_from_root_key(TEST_ROOT_KEY, coin, index).is_ok(),
                    "Expected to generate a key pair for index {}",
                    index
                )
            }
        }
    }

    #[test]
    fn generates_correct_eth_address() {
        assert_eq!(
            &generate_eth_address(TEST_ROOT_KEY, 0).unwrap().to_string(),
            "0x48575a3C8fa7D0469FD39eCB67ec68d8C7564637"
        );
        assert_eq!(
            &generate_eth_address(TEST_ROOT_KEY, 1).unwrap().to_string(),
            "0xB46878bd2E68e2b3f5145ccB868E626572905c5F"
        );
    }

    #[test]
    fn generates_correct_btc_address() {
        // === p2wpkh - pay-to-witness-pubkey-hash (segwit) addresses ===
        assert_eq!(
            generate_btc_address_from_index(TEST_ROOT_KEY, 0, true, P2wpkh, Network::Mainnet)
                .unwrap(),
            "bc1qawvxp3jxlzj3ydcfjyq83cxkdxpu7st8az5hvq"
        );

        // testnet generates different addresses to mainnet
        assert_eq!(
            generate_btc_address_from_index(TEST_ROOT_KEY, 0, true, P2wpkh, Network::Testnet)
                .unwrap(),
            "tb1qawvxp3jxlzj3ydcfjyq83cxkdxpu7st8hy0yhn"
        );

        assert_eq!(
            generate_btc_address_from_index(TEST_ROOT_KEY, 1, true, P2wpkh, Network::Mainnet)
                .unwrap(),
            "bc1q6uq0qny5pel4aane4cj0kuqz5sgkxczv6y4ypy"
        );

        // === p2pkh - pay-to-pubkey-hash (legacy) addresses ===
        assert_eq!(
            generate_btc_address_from_index(TEST_ROOT_KEY, 0, false, P2pkh, Network::Mainnet)
                .unwrap(),
            "1Q6hHytu6sZmib3TUNeEhGxE8L2ydx5JZo",
        );

        // testnet generates different addresses to mainnet
        assert_eq!(
            generate_btc_address_from_index(TEST_ROOT_KEY, 0, false, P2pkh, Network::Testnet)
                .unwrap(),
            "n4ceb2ysuu12VhX5BwccXCAYzKdgZY2XFH",
        );

        assert_eq!(
            generate_btc_address_from_index(TEST_ROOT_KEY, 1, true, P2pkh, Network::Mainnet)
                .unwrap(),
            "1LbqQTsn9EJN1yWJ2YkQGtaihovjgs6cfW"
        );

        assert_eq!(
            generate_btc_address_from_index(TEST_ROOT_KEY, 1, false, P2pkh, Network::Mainnet)
                .unwrap(),
            "1PWyfwtkS9co1rTHvU2SSESbcu6zi2TmxH"
        );

        assert_ne!(
            generate_btc_address_from_index(TEST_ROOT_KEY, 2, false, P2pkh, Network::Mainnet)
                .unwrap(),
            "1LbqQTsn9EJN1yWJ2YkQGtaihovjgs6cfW"
        );

        assert!(generate_btc_address_from_index(
            "not a real key",
            4,
            false,
            P2pkh,
            Network::Mainnet
        )
        .is_err())
    }
}
