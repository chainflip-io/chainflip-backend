use crate::{
    constants::ETH_TO_WEI_FACTOR,
    eth::{rpc::EthRpcApi, EventParseError},
};
use anyhow::Result;
use sp_core::Hasher;
use sp_runtime::traits::Keccak256;
use web3::{
    contract::tokens::Tokenizable,
    ethabi::Log,
    types::{Address, BlockId},
};

/// Helper method to decode the parameters from an ETH log
pub fn decode_log_param<T: Tokenizable>(log: &Log, param_name: &str) -> Result<T> {
    let token = &log
        .params
        .iter()
        .find(|&p| p.name == param_name)
        .ok_or_else(|| EventParseError::MissingParam(String::from(param_name)))?
        .value;

    Ok(Tokenizable::from_token(token.clone())?)
}

/// Get a eth address from a public key
pub fn pubkey_to_eth_addr(pubkey: secp256k1::PublicKey) -> [u8; 20] {
    let pubkey_bytes: [u8; 64] = pubkey.serialize_uncompressed()[1..]
        .try_into()
        .expect("Should be a valid pubkey");

    let pubkey_hash = Keccak256::hash(&pubkey_bytes);

    // take the last 160bits (20 bytes)
    let addr: [u8; 20] = pubkey_hash[12..]
        .try_into()
        .expect("Should be exactly 20 bytes long");

    addr
}

#[cfg(test)]
mod utils_tests {
    use super::*;
    use secp256k1::PublicKey;
    use std::str::FromStr;

    #[test]
    fn test_pubkey_to_eth_addr() {
        // The secret key and corresponding eth addr were taken from an example in the "Mastering Ethereum" Book.
        let sk = secp256k1::SecretKey::from_str(
            "f8f8a2f43c8376ccb0871305060d7b27b0554d2cc72bccf41b2705608452f315",
        )
        .unwrap();

        let pk = PublicKey::from_secret_key(&secp256k1::Secp256k1::signing_only(), &sk);

        let expected: [u8; 20] = hex::decode("001d3f1ef827552ae1114027bd3ecf1f086ba0f9")
            .unwrap()
            .try_into()
            .unwrap();

        assert_eq!(pubkey_to_eth_addr(pk), expected);
    }
}

pub async fn log_transactions<EthRpc>(
    block: BlockId,
    monitored_addresses: &[Address],
    eth_rpc: &EthRpc,
    logger: &slog::Logger,
) -> Result<()>
where
    EthRpc: EthRpcApi,
{
    let block = eth_rpc.block_with_txs(block).await?;
    for tx in &block.transactions {
        if let Some(tx_to) = tx.to {
            monitored_addresses.iter().for_each(|address| {
                if tx_to == *address {
                    slog::info!(
                        &logger,
                        "Observed transaction of {:?} ETH to {:?}",
                        (tx.value.as_u128() as f64) / (ETH_TO_WEI_FACTOR as f64),
                        address
                    );
                }
            });
        }
    }
    Ok(())
}
