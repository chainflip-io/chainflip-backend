use crate::eth::EventProducerError;
use anyhow::Result;
use sp_core::Hasher;
use sp_runtime::traits::Keccak256;
use std::convert::TryInto;
use web3::{contract::tokens::Tokenizable, ethabi::Log};

/// Helper method to decode the parameters from an ETH log
pub fn decode_log_param<T: Tokenizable>(log: &Log, param_name: &str) -> Result<T> {
    let token = &log
        .params
        .iter()
        .find(|&p| p.name == param_name)
        .ok_or_else(|| EventProducerError::MissingParam(String::from(param_name)))?
        .value;

    Ok(Tokenizable::from_token(token.clone())?)
}

/// Get a eth address from a public key
pub fn pubkey_to_eth_addr(r: secp256k1::PublicKey) -> [u8; 20] {
    let v_pub: [u8; 64] = r.serialize_uncompressed()[1..]
        .try_into()
        .expect("Should be a valid pubkey");

    let pubkey_hash = Keccak256::hash(&v_pub).as_bytes().to_owned();

    // take the last 160bits (20 bytes)
    let addr: [u8; 20] = pubkey_hash[12..]
        .try_into()
        .expect("Should only be 20 bytes long");

    return addr;
}

#[cfg(test)]
mod utils_tests {
    use super::*;
    use secp256k1::PublicKey;
    use std::str::FromStr;

    #[test]
    fn test_pubkey_to_eth_addr() {
        // The secret key and corresponding eth addr were taken from an example in the Ethereum Book.
        let sk_1 = secp256k1::SecretKey::from_str(
            "f8f8a2f43c8376ccb0871305060d7b27b0554d2cc72bccf41b2705608452f315",
        )
        .unwrap();

        let pk_2 = PublicKey::from_secret_key(&secp256k1::Secp256k1::signing_only(), &sk_1);

        let expected: [u8; 20] = hex::decode("001d3f1ef827552ae1114027bd3ecf1f086ba0f9")
            .unwrap()
            .try_into()
            .unwrap();

        assert_eq!(pubkey_to_eth_addr(pk_2), expected);
    }
}
