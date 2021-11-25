//! Contains the information required to use the KeyManager contract as a source for
//! the EthEventStreamer

use crate::eth::SharedEvent;
use crate::state_chain::client::StateChainClient;
use crate::{
    eth::{utils, SignatureAndEvent},
    state_chain::client::StateChainRpcApi,
};
use cf_chains::ChainId;
use std::sync::Arc;
use web3::{
    contract::tokens::Tokenizable,
    ethabi::{self, RawLog, Token},
    types::{H160, H256},
};

use anyhow::Result;

use std::fmt::Debug;

use async_trait::async_trait;

use super::decode_shared_event_closure;
use super::event_common::EventWithCommon;
use super::EthObserver;

/// A wrapper for the KeyManager Ethereum contract.
pub struct KeyManager {
    pub deployed_address: H160,
    pub contract: ethabi::Contract,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ChainflipKey {
    pub_key_x: ethabi::Uint,
    pub_key_y_parity: ethabi::Uint,
}

impl ChainflipKey {
    /// Create a ChainflipKey from a decimal string
    pub fn from_dec_str(dec_str: &str, parity: bool) -> Result<Self> {
        let pub_key_x = web3::types::U256::from_dec_str(dec_str)?;
        Ok(ChainflipKey {
            pub_key_x,
            pub_key_y_parity: match parity {
                true => web3::types::U256::from_dec_str("1").unwrap(),
                false => web3::types::U256::from_dec_str("0").unwrap(),
            },
        })
    }

    /// 1 byte of pub_key_y_parity followed by 32 bytes of pub_key_x
    /// Equivalent to secp256k1::PublicKey.serialize()
    pub fn serialize(&self) -> [u8; 33] {
        let mut bytes: [u8; 33] = [0; 33];
        self.pub_key_x.to_big_endian(&mut bytes[1..]);
        bytes[0] = match self.pub_key_y_parity.is_zero() {
            true => 2,
            false => 3,
        };
        bytes
    }
}

impl Tokenizable for ChainflipKey {
    fn from_token(token: ethabi::Token) -> Result<Self, web3::contract::Error>
    where
        Self: Sized,
    {
        if let Token::Tuple(members) = token {
            if members.len() != 2 {
                Err(web3::contract::Error::InvalidOutputType(
                    stringify!(ChainflipKey).to_owned(),
                ))
            } else {
                Ok(ChainflipKey {
                    pub_key_x: ethabi::Uint::from_token(members[0].clone())?,
                    pub_key_y_parity: ethabi::Uint::from_token(members[1].clone())?,
                })
            }
        } else {
            Err(web3::contract::Error::InvalidOutputType(
                stringify!(ChainflipKey).to_owned(),
            ))
        }
    }

    fn into_token(self) -> ethabi::Token {
        Token::Tuple(vec![
            // Key
            Token::Uint(self.pub_key_x),
            Token::Uint(self.pub_key_y_parity),
        ])
    }
}

/// Represents the events that are expected from the KeyManager contract.
#[derive(Debug)]
pub enum KeyManagerEvent {
    /// `AggKeySetByAggKey(Key oldKey, Key newKey)`
    AggKeySetByAggKey {
        /// The old key.
        old_key: ChainflipKey,
        /// The new key.
        new_key: ChainflipKey,
    },

    /// `AggKeySetByGovKey(Key oldKey, Key newKey)`
    AggKeySetByGovKey {
        /// The old key.
        old_key: ChainflipKey,
        /// The new key.
        new_key: ChainflipKey,
    },

    /// `GovKeySetByGovKey(Key oldKey, Key newKey)`
    GovKeySetByGovKey {
        /// The old key.
        old_key: ChainflipKey,
        /// The new key.
        new_key: ChainflipKey,
    },

    /// Events that both the Key and Stake Manager contracts can output (Shared.sol)
    Shared(SharedEvent),
}

#[async_trait]
impl EthObserver for KeyManager {
    type EventParameters = KeyManagerEvent;

    async fn handle_event<RPCClient>(
        &self,
        event: EventWithCommon<Self::EventParameters>,
        state_chain_client: Arc<StateChainClient<RPCClient>>,
        logger: &slog::Logger,
    ) where
        RPCClient: 'static + StateChainRpcApi + Sync + Send,
    {
        match event.event_parameters {
            KeyManagerEvent::AggKeySetByAggKey { new_key, .. }
            | KeyManagerEvent::AggKeySetByGovKey { new_key, .. }
            | KeyManagerEvent::GovKeySetByGovKey { new_key, .. } => {
                let _ = state_chain_client
                    .submit_extrinsic(
                        logger,
                        pallet_cf_witnesser_api::Call::witness_vault_key_rotated(
                            ChainId::Ethereum,
                            new_key.serialize().to_vec(),
                            event.block_number,
                            event.tx_hash.to_vec(),
                        ),
                    )
                    .await;
            }
            KeyManagerEvent::Shared(shared_event) => match shared_event {
                SharedEvent::Refunded { .. } => {}
                SharedEvent::RefundFailed { .. } => {}
            },
        }
    }

    fn decode_log_closure(
        &self,
    ) -> Result<Box<dyn Fn(H256, ethabi::RawLog) -> Result<Self::EventParameters> + Send>> {
        let ak_set_ak = SignatureAndEvent::new(&self.contract, "AggKeySetByAggKey")?;
        let ak_set_gk = SignatureAndEvent::new(&self.contract, "AggKeySetByGovKey")?;
        let gk_set_gk = SignatureAndEvent::new(&self.contract, "GovKeySetByGovKey")?;

        let decode_shared_event_closure = decode_shared_event_closure(&self.contract)?;

        Ok(Box::new(
            move |signature: H256, raw_log: RawLog| -> Result<KeyManagerEvent> {
                Ok(if signature == ak_set_ak.signature {
                    let log = ak_set_ak.event.parse_log(raw_log)?;
                    KeyManagerEvent::AggKeySetByAggKey {
                        old_key: utils::decode_log_param::<ChainflipKey>(&log, "oldKey")?,
                        new_key: utils::decode_log_param::<ChainflipKey>(&log, "newKey")?,
                    }
                } else if signature == ak_set_gk.signature {
                    let log = ak_set_gk.event.parse_log(raw_log)?;
                    KeyManagerEvent::AggKeySetByGovKey {
                        old_key: utils::decode_log_param::<ChainflipKey>(&log, "oldKey")?,
                        new_key: utils::decode_log_param::<ChainflipKey>(&log, "newKey")?,
                    }
                } else if signature == gk_set_gk.signature {
                    let log = gk_set_gk.event.parse_log(raw_log)?;
                    KeyManagerEvent::GovKeySetByGovKey {
                        old_key: utils::decode_log_param::<ChainflipKey>(&log, "oldKey")?,
                        new_key: utils::decode_log_param::<ChainflipKey>(&log, "newKey")?,
                    }
                } else {
                    KeyManagerEvent::Shared(decode_shared_event_closure(signature, raw_log)?)
                })
            },
        ))
    }

    fn get_deployed_address(&self) -> H160 {
        self.deployed_address
    }
}

impl KeyManager {
    /// Loads the contract abi to get the event definitions
    pub fn new(deployed_address: H160) -> Result<Self> {
        Ok(Self {
            deployed_address,
            contract: ethabi::Contract::load(std::include_bytes!("abis/KeyManager.json").as_ref())?,
        })
    }
}

#[cfg(test)]
mod tests {

    use crate::eth::EventParseError;

    use super::*;
    use hex;
    use std::str::FromStr;
    use web3::types::H256;

    #[test]
    fn test_key_change_parsing() {
        // All log data for these tests was obtained from the events in the `deploy_and` script:
        // https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/scripts/deploy_and.py

        // All the key strings in this test are decimal pub keys derived from the priv keys in the consts.py script
        // https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/tests/consts.py

        let key_manager = KeyManager::new(H160::default()).unwrap();

        let decode_log = key_manager.decode_log_closure().unwrap();

        {
            // 🔑 Aggregate Key sets the new Aggregate Key 🔑
            let event_signature = H256::from_str(
                "0x5cba64f32f2576e404f74394dc04611cce7416e299c94db0667d4e315e852521",
            )
            .unwrap();

            match decode_log(
                event_signature,
                RawLog {
                    topics : vec![event_signature],
                    data : hex::decode("31b2ba4b46201610901c5164f42edd1f64ce88076fde2e2c544f9dc3d7b350ae00000000000000000000000000000000000000000000000000000000000000011742daacd4dbfbe66d4c8965550295873c683cb3b65019d3a53975ba553cc31d0000000000000000000000000000000000000000000000000000000000000001").unwrap()
                }
            ).expect("Failed parsing AGG_SET_AGG_LOG event") {
                KeyManagerEvent::AggKeySetByAggKey {
                    old_key,
                    new_key,
                } => {
                    assert_eq!(old_key, ChainflipKey::from_dec_str("22479114112312168431982914496826057754130808976066989807481484372215659188398",true).unwrap());
                    assert_eq!(new_key, ChainflipKey::from_dec_str("10521316663921629387264629518161886172223783929820773409615991397525613232925",true).unwrap());
                }
                _ => panic!("Expected KeyManagerEvent::KeyChange, got different variant"),
            }
        }

        // 🔑 Governance Key sets the new Aggregate Key 🔑
        {
            let event_signature = H256::from_str(
                "0xe441a6cf7a12870075eb2f6399c0de122bfe6cd8a75bfa83b05d5b611552532e",
            )
            .unwrap();

            match decode_log(
                event_signature,
                RawLog {
                    topics : vec![event_signature],
                    data : hex::decode("1742daacd4dbfbe66d4c8965550295873c683cb3b65019d3a53975ba553cc31d000000000000000000000000000000000000000000000000000000000000000131b2ba4b46201610901c5164f42edd1f64ce88076fde2e2c544f9dc3d7b350ae0000000000000000000000000000000000000000000000000000000000000001").unwrap()
                }
            ).expect("Failed parsing GOV_SET_AGG_LOG event")
            {
                KeyManagerEvent::AggKeySetByGovKey {
                    old_key,
                    new_key,
                } => {
                    assert_eq!(old_key, ChainflipKey::from_dec_str("10521316663921629387264629518161886172223783929820773409615991397525613232925",true).unwrap());
                    assert_eq!(new_key, ChainflipKey::from_dec_str("22479114112312168431982914496826057754130808976066989807481484372215659188398",true).unwrap());
                }
                _ => panic!("Expected KeyManagerEvent::KeyChange, got different variant"),
            }
        }

        // 🔑 Governance Key sets the new Governance Key 🔑
        {
            let event_signature = H256::from_str(
                "0x92b56cc56b503f112edd042b017b7050418910551d37acba075d429fc28adbb4",
            )
            .unwrap();

            match decode_log(
                event_signature,
                RawLog {
                    topics : vec![event_signature],
                    data : hex::decode("423ebe9d54bf7cb10dfebe2b323bb9a01bfede660619a7f49531c96a23263dd800000000000000000000000000000000000000000000000000000000000000014e3d72babbee4133675d42db3bba62a7dfbc47a91ddc5db56d95313d908c08f80000000000000000000000000000000000000000000000000000000000000000").unwrap()
                }
            ).expect("Failed parsing GOV_SET_GOV_LOG event")
            {
                KeyManagerEvent::GovKeySetByGovKey {
                    old_key,
                    new_key,
                } => {
                    assert_eq!(old_key, ChainflipKey::from_dec_str("29963508097954364125322164523090632495724997135004046323041274775773196467672",true).unwrap());
                    assert_eq!(new_key, ChainflipKey::from_dec_str("35388971693871284788334991319340319470612669764652701045908837459480931993848",false).unwrap());
                }
                _ => panic!("Expected KeyManagerEvent::KeyChange, got different variant"),
            }
        }

        // Invalid sig test
        {
            let invalid_signature = H256::from_str(
                "0x0b0b5ed18390ab49777844d5fcafb9865c74095ceb3e73cc57d1fbcc926103b5",
            )
            .unwrap();
            let res = decode_log(
                invalid_signature,
                RawLog {
                    topics : vec![invalid_signature],
                    data : hex::decode("000000000000000000000000000000000000000000000000000000000000000131b2ba4b46201610901c5164f42edd1f64ce88076fde2e2c544f9dc3d7b350ae00000000000000000000000000000000000000000000000000000000000000011742daacd4dbfbe66d4c8965550295873c683cb3b65019d3a53975ba553cc31d0000000000000000000000000000000000000000000000000000000000000001").unwrap()
                }
            )
            .map_err(|e| match e.downcast_ref::<EventParseError>() {
                Some(EventParseError::UnexpectedEvent(_)) => {}
                _ => {
                    panic!("Incorrect error parsing INVALID_SIG_LOG");
                }
            });
            assert!(res.is_err());
        }
    }

    #[test]
    fn refunded_log_parsing() {
        let key_manager = KeyManager::new(H160::default()).unwrap();
        let decode_log = key_manager.decode_log_closure().unwrap();

        let refunded_event_signature =
            H256::from_str("0x3d2a04f53164bedf9a8a46353305d6b2d2261410406df3b41f99ce6489dc003c")
                .unwrap();

        match decode_log(
            refunded_event_signature,
            RawLog {
                topics: vec![refunded_event_signature],
                data: hex::decode(
                    "00000000000000000000000000000000000000000000000000000a1eaa1e2544",
                )
                .unwrap(),
            },
        )
        .unwrap()
        {
            KeyManagerEvent::Shared(SharedEvent::Refunded { amount }) => {
                assert_eq!(11126819398980, amount);
            }
            _ => panic!("Expected KeyManager::Refunded, got a different variant"),
        }
    }

    #[tokio::test]
    async fn common_event_info_decoded_correctly() {
        let key_manager = KeyManager::new(H160::default()).unwrap();

        let transaction_hash =
            H256::from_str("0x621aebbe0bb116ae98d36a195ad8df4c5e7c8785fae5823f5f1fe1b691e91bf2")
                .unwrap();

        let event = EventWithCommon::decode(
            &key_manager.decode_log_closure().unwrap(),
             web3::types::Log {
                address: H160::zero(),
                topics: vec![H256::from_str("0x5cba64f32f2576e404f74394dc04611cce7416e299c94db0667d4e315e852521")
                .unwrap()],
                data: web3::types::Bytes(hex::decode("31b2ba4b46201610901c5164f42edd1f64ce88076fde2e2c544f9dc3d7b350ae00000000000000000000000000000000000000000000000000000000000000011742daacd4dbfbe66d4c8965550295873c683cb3b65019d3a53975ba553cc31d0000000000000000000000000000000000000000000000000000000000000001").unwrap()),
                block_hash: None,
                block_number: Some(web3::types::U64::zero()),
                transaction_hash: Some(transaction_hash),
                transaction_index: None,
                log_index: None,
                transaction_log_index: None,
                log_type: None,
                removed: None,
            }
        ).unwrap();

        assert_eq!(event.tx_hash, transaction_hash.to_fixed_bytes());
    }

    #[test]
    fn test_chainflip_key_serialize() {
        use secp256k1::PublicKey;

        // Create a `ChainflipKey` and a `PublicKey` that are the same
        let cf_key = ChainflipKey::from_dec_str(
            "22479114112312168431982914496826057754130808976066989807481484372215659188398",
            true,
        )
        .unwrap();

        let sk = secp256k1::SecretKey::from_str(
            "fbcb47bc85b881e0dfb31c872d4e06848f80530ccbd18fc016a27c4a744d0eba",
        )
        .unwrap();

        let secp_key = PublicKey::from_secret_key(&secp256k1::Secp256k1::signing_only(), &sk);

        // Compare the serialize() values to make sure we serialize the same as secp256k1
        assert_eq!(cf_key.serialize(), secp_key.serialize());
    }
}
