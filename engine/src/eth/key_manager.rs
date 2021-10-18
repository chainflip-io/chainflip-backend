//! Contains the information required to use the KeyManager contract as a source for
//! the EthEventStreamer

use crate::eth::SharedEvent;
use crate::state_chain::client::StateChainClient;
use crate::{
    eth::{eth_event_streamer, utils, SignatureAndEvent},
    logging::COMPONENT_KEY,
    settings,
};
use std::sync::Arc;
use web3::{
    contract::tokens::Tokenizable,
    ethabi::{self, RawLog, Token},
    transports::WebSocket,
    types::{H160, H256},
    Web3,
};

use anyhow::Result;

use futures::{Future, Stream, StreamExt};

use slog::o;

use super::decode_shared_event_closure;
use super::eth_event_streamer::Event;

/// Set up the eth event streamer for the KeyManager contract, and start it
pub async fn start_key_manager_witness(
    web3: &Web3<WebSocket>,
    settings: &settings::Settings,
    _state_chain_client: Arc<StateChainClient>,
    logger: &slog::Logger,
) -> Result<impl Future> {
    let logger = logger.new(o!(COMPONENT_KEY => "KeyManagerWitness"));
    slog::info!(logger, "Starting KeyManager witness");

    slog::info!(logger, "Load Contract ABI");
    let key_manager = KeyManager::new(&settings)?;

    let mut event_stream = key_manager
        .event_stream(&web3, settings.eth.from_block, &logger)
        .await?;

    Ok(async move {
        while let Some(result_event) = event_stream.next().await {
            // TODO: Handle unwraps
            match result_event.unwrap() {
                KeyManagerEvent::AggKeySetByAggKey { tx_hash, .. } => {
                    slog::info!(
                        logger,
                        "AggKeySetByAggKey event found: {}",
                        hex::encode(tx_hash)
                    );
                }
                KeyManagerEvent::AggKeySetByGovKey { tx_hash, .. } => {
                    slog::info!(
                        logger,
                        "AggKeySetByGovKey event found: {}",
                        hex::encode(tx_hash)
                    );
                }
                KeyManagerEvent::GovKeySetByGovKey { tx_hash, .. } => {
                    slog::info!(
                        logger,
                        "GovKeySetByGovKey event found: {}",
                        hex::encode(tx_hash)
                    );
                }
                KeyManagerEvent::Refunded { tx_hash, .. } => {
                    slog::info!(logger, "Refunded event found: {}", hex::encode(tx_hash));
                }
                KeyManagerEvent::Shared(shared_event) => match shared_event {
                    SharedEvent::Refunded { .. } => {
                        slog::info!(
                            logger,
                            "Refunded event found: {}",
                            hex::encode(event.tx_hash)
                        );
                    }
                    SharedEvent::RefundFailed { .. } => {
                        slog::info!(
                            logger,
                            "RefundFailed event found: {}",
                            hex::encode(event.tx_hash)
                        );
                    }
                },
            }
        }
    })
}

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
        /// Tx hash of the tx that created the event
        tx_hash: [u8; 32],
    },

    /// `AggKeySetByGovKey(Key oldKey, Key newKey)`
    AggKeySetByGovKey {
        /// The old key.
        old_key: ChainflipKey,
        /// The new key.
        new_key: ChainflipKey,
        /// Tx hash of the tx that created the event
        tx_hash: [u8; 32],
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

impl KeyManager {
    /// Loads the contract abi to get event definitions
    pub fn new(settings: &settings::Settings) -> Result<Self> {
        Ok(Self {
            deployed_address: settings.eth.key_manager_eth_address,
            contract: ethabi::Contract::load(std::include_bytes!("abis/KeyManager.json").as_ref())?,
        })
    }

    // TODO: Maybe try to factor this out (See StakeManager)
    pub async fn event_stream(
        &self,
        web3: &Web3<WebSocket>,
        from_block: u64,
        logger: &slog::Logger,
    ) -> Result<impl Stream<Item = Result<Event<KeyManagerEvent>>>> {
        slog::info!(logger, "Creating new event stream");
        eth_event_streamer::new_eth_event_stream(
            web3,
            self.deployed_address,
            self.decode_log_closure()?,
            from_block,
            logger,
        )
        .await
    }

    pub fn decode_log_closure(
        &self,
    ) -> Result<impl Fn(H256, H256, RawLog) -> Result<KeyManagerEvent>> {
        let ak_set_ak = SignatureAndEvent::new(&self.contract, "AggKeySetByAggKey")?;
        let ak_set_gk = SignatureAndEvent::new(&self.contract, "AggKeySetByGovKey")?;
        let gk_set_gk = SignatureAndEvent::new(&self.contract, "GovKeySetByGovKey")?;
        let refunded = SignatureAndEvent::new(&self.contract, "Refunded")?;

        Ok(
            move |signature: H256, tx_hash: H256, raw_log: RawLog| -> Result<KeyManagerEvent> {
                let tx_hash = tx_hash.to_fixed_bytes();
                if signature == ak_set_ak.signature {
                    let log = ak_set_ak.event.parse_log(raw_log)?;
                    let event = KeyManagerEvent::AggKeySetByAggKey {
                        old_key: utils::decode_log_param::<ChainflipKey>(&log, "oldKey")?,
                        new_key: utils::decode_log_param::<ChainflipKey>(&log, "newKey")?,
                        tx_hash,
                    };
                    Ok(event)
                } else if signature == ak_set_gk.signature {
                    let log = ak_set_gk.event.parse_log(raw_log)?;
                    let event = KeyManagerEvent::AggKeySetByGovKey {
                        old_key: utils::decode_log_param::<ChainflipKey>(&log, "oldKey")?,
                        new_key: utils::decode_log_param::<ChainflipKey>(&log, "newKey")?,
                        tx_hash,
                    };
                    Ok(event)
                } else if signature == gk_set_gk.signature {
                    let log = gk_set_gk.event.parse_log(raw_log)?;
                    let event = KeyManagerEvent::GovKeySetByGovKey {
                        old_key: utils::decode_log_param::<ChainflipKey>(&log, "oldKey")?,
                        new_key: utils::decode_log_param::<ChainflipKey>(&log, "newKey")?,
                    })
                } else {
                    Ok(KeyManagerEvent::Shared(decode_shared_event_closure(
                        signature, raw_log,
                    )?))
                }
            },
        )
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

        // All the key strings in this test are decimal pub keys from the priv keys in the consts.py script
        // https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/tests/consts.py

        let settings = settings::test_utils::new_test_settings().unwrap();

        let key_manager = KeyManager::new(&settings).unwrap();

        let decode_log = key_manager.decode_log_closure().unwrap();

        {
            // 🔑 Aggregate Key sets the new Aggregate Key 🔑
            let event_signature = H256::from_str(
                "0x5cba64f32f2576e404f74394dc04611cce7416e299c94db0667d4e315e852521",
            )
            .unwrap();

            let transaction_hash = H256::from_str(
                "0x621aebbe0bb116ae98d36a195ad8df4c5e7c8785fae5823f5f1fe1b691e91bf2",
            )
            .unwrap();
            match decode_log(
                event_signature,
                transaction_hash,
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

            let transaction_hash = H256::from_str(
                "0x76499f35052e9f09a5a12969abae380df638f809cc258869a29320197a6335b5",
            )
            .unwrap();
            match decode_log(
                event_signature,
                transaction_hash,
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

            let transaction_hash = H256::from_str(
                "0xd962552749780fe5e13b2b0aec12ab4d806a1f2d04432b5bad2715a800268421",
            )
            .unwrap();
            match decode_log(
                event_signature,
                transaction_hash,
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
        let settings = settings::test_utils::new_test_settings().unwrap();

        let key_manager = KeyManager::new(&settings).unwrap();
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
        let settings = settings::test_utils::new_test_settings().unwrap();

        let key_manager = KeyManager::new(&settings).unwrap();

        let transaction_hash =
            H256::from_str("0x6320cfd702415644192bf57702ceccc0d6de0ddc54fe9aa53f9b1a5d9035fe52")
                .unwrap();

        let event = Event::decode(
            &key_manager.decode_log_closure().unwrap(),
             web3::types::Log {
                address: H160::zero(),
                topics: vec![H256::from_str("0x19389c59b816d8b0ec43f2d5ed9b41bddc63d66dac1ecd808efe35b86b9ee0bf")
                .unwrap()],
                data: web3::types::Bytes(hex::decode("00000000000000000000000000000000000000000000000000000000000000001742daacd4dbfbe66d4c8965550295873c683cb3b65019d3a53975ba553cc31d000000000000000000000000000000000000000000000000000000000000000131b2ba4b46201610901c5164f42edd1f64ce88076fde2e2c544f9dc3d7b350ae0000000000000000000000000000000000000000000000000000000000000001").unwrap()),
                block_hash: None,
                block_number: None,
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
}
