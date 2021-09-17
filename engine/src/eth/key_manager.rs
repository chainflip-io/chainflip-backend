//! Contains the information required to use the KeyManager contract as a source for
//! the EthEventStreamer

use crate::{
    eth::{eth_event_streamer, utils, EventParseError, SignatureAndEvent},
    logging::COMPONENT_KEY,
    settings,
    state_chain::runtime::StateChainRuntime,
};
use std::sync::Arc;
use substrate_subxt::{Client, PairSigner};
use tokio::sync::Mutex;
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

/// Set up the eth event streamer for the KeyManager contract, and start it
pub async fn start_key_manager_witness(
    web3: &Web3<WebSocket>,
    settings: &settings::Settings,
    _signer: Arc<Mutex<PairSigner<StateChainRuntime, sp_core::sr25519::Pair>>>,
    _subxt_client: Client<StateChainRuntime>,
    logger: &slog::Logger,
) -> Result<impl Future> {
    let logger = logger.new(o!(COMPONENT_KEY => "KeyManagerWitness"));
    slog::info!(logger, "Starting KeyManager witness");

    slog::info!(logger, "Load Contract ABI");
    let key_manager = KeyManager::new(&settings)?;

    slog::info!(logger, "Creating Event Stream");
    let mut event_stream = key_manager
        .event_stream(&web3, settings.eth.from_block, &logger)
        .await?;

    Ok(async move {
        while let Some(result_event) = event_stream.next().await {
            match result_event.unwrap() {
                // TODO: Handle unwraps
                KeyManagerEvent::KeyChange { .. } => {
                    todo!();
                }
                KeyManagerEvent::Refunded { amount } => todo!("Refunded({})", amount),
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
    /// `Staked(nodeId, amount)`
    KeyChange {
        /// Whether the change was signed by the AggKey.
        signed: bool,
        /// The old key.
        old_key: ChainflipKey,
        /// The new key.
        new_key: ChainflipKey,
        /// Transaction hash that created the event
        tx_hash: [u8; 32],
    },

    /// `Refunded(amount)`
    Refunded {
        /// The amount of ETH refunded
        amount: u128,
    },
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
    ) -> Result<impl Stream<Item = Result<KeyManagerEvent>>> {
        let event_stream = eth_event_streamer::new_eth_event_stream(
            web3,
            self.deployed_address,
            self.decode_log_closure()?,
            from_block,
            logger,
        )
        .await;
        println!("Initialise event stream");
        return event_stream;
    }

    pub fn decode_log_closure(
        &self,
    ) -> Result<impl Fn(H256, H256, RawLog) -> Result<KeyManagerEvent>> {
        let key_change = SignatureAndEvent::new(&self.contract, "KeyChange")?;
        let refunded = SignatureAndEvent::new(&self.contract, "Refunded")?;

        Ok(
            move |signature: H256, tx_hash: H256, raw_log: RawLog| -> Result<KeyManagerEvent> {
                let tx_hash = tx_hash.to_fixed_bytes();
                if signature == key_change.signature {
                    let log = key_change.event.parse_log(raw_log)?;
                    let event = KeyManagerEvent::KeyChange {
                        signed: utils::decode_log_param::<bool>(&log, "signedByAggKey")?,
                        old_key: utils::decode_log_param::<ChainflipKey>(&log, "oldKey")?,
                        new_key: utils::decode_log_param::<ChainflipKey>(&log, "newKey")?,
                        tx_hash,
                    };
                    Ok(event)
                } else if signature == refunded.signature {
                    let log = refunded.event.parse_log(raw_log)?;
                    let event = KeyManagerEvent::Refunded {
                        amount: utils::decode_log_param::<ethabi::Uint>(&log, "amount")?.as_u128(),
                    };
                    Ok(event)
                } else {
                    Err(anyhow::Error::from(EventParseError::UnexpectedEvent(
                        signature,
                    )))
                }
            },
        )
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use hex;
    use std::str::FromStr;
    use web3::types::H256;

    #[test]
    fn test_key_change_parsing() {
        // All log data for these tests was obtained from the events in the `deploy_and` script:
        // https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/scripts/deploy_and.py

        // All the key strings in this test are decimal versions of the hex strings in the consts.py script
        // https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/tests/consts.py
        // TODO: Use hex strings instead of dec strings. So we can use the exact const hex strings from consts.py.

        let settings = settings::test_utils::new_test_settings().unwrap();

        let key_manager = KeyManager::new(&settings).unwrap();

        let decode_log = key_manager.decode_log_closure().unwrap();

        let key_change_event_signature =
            H256::from_str("0x19389c59b816d8b0ec43f2d5ed9b41bddc63d66dac1ecd808efe35b86b9ee0bf")
                .unwrap();

        // 🔑 Aggregate Key sets the new Aggregate Key 🔑
        {
            let transaction_hash = H256::from_str(
                "0x04629152b064c0d1343161c43f3b78cf67e9be35fc97f66bbb0e1ca1a0206bae",
            )
            .unwrap();
            match decode_log(
                key_change_event_signature,
                transaction_hash,
                RawLog {
                    topics : vec![key_change_event_signature],
                    data : hex::decode("000000000000000000000000000000000000000000000000000000000000000131b2ba4b46201610901c5164f42edd1f64ce88076fde2e2c544f9dc3d7b350ae00000000000000000000000000000000000000000000000000000000000000011742daacd4dbfbe66d4c8965550295873c683cb3b65019d3a53975ba553cc31d0000000000000000000000000000000000000000000000000000000000000001").unwrap()
                }
            ).expect("Failed parsing AGG_SET_AGG_LOG event") {
                KeyManagerEvent::KeyChange {
                    signed,
                    old_key,
                    new_key,
                    tx_hash,
                } => {
                    assert_eq!(signed, true);
                    assert_eq!(old_key, ChainflipKey::from_dec_str("22479114112312168431982914496826057754130808976066989807481484372215659188398",true).unwrap());
                    assert_eq!(new_key, ChainflipKey::from_dec_str("10521316663921629387264629518161886172223783929820773409615991397525613232925",true).unwrap());

                    assert_eq!(tx_hash, transaction_hash.to_fixed_bytes());
                }
                _ => panic!("Expected KeyManagerEvent::KeyChange, got different variant"),
            }
        }

        // 🔑 Governance Key sets the new Aggregate Key 🔑
        {
            let transaction_hash = H256::from_str(
                "0x6320cfd702415644192bf57702ceccc0d6de0ddc54fe9aa53f9b1a5d9035fe52",
            )
            .unwrap();
            match decode_log(
                key_change_event_signature,
                transaction_hash,
                RawLog {
                    topics : vec![key_change_event_signature],
                    data : hex::decode("00000000000000000000000000000000000000000000000000000000000000001742daacd4dbfbe66d4c8965550295873c683cb3b65019d3a53975ba553cc31d000000000000000000000000000000000000000000000000000000000000000131b2ba4b46201610901c5164f42edd1f64ce88076fde2e2c544f9dc3d7b350ae0000000000000000000000000000000000000000000000000000000000000001").unwrap()
                }
            ).expect("Failed parsing GOV_SET_AGG_LOG event")
            {
                KeyManagerEvent::KeyChange {
                    signed,
                    old_key,
                    new_key,
                    tx_hash,
                } => {
                    assert_eq!(signed, false);
                    assert_eq!(old_key, ChainflipKey::from_dec_str("10521316663921629387264629518161886172223783929820773409615991397525613232925",true).unwrap());
                    assert_eq!(new_key, ChainflipKey::from_dec_str("22479114112312168431982914496826057754130808976066989807481484372215659188398",true).unwrap());

                    assert_eq!(tx_hash, transaction_hash.to_fixed_bytes());
                }
                _ => panic!("Expected KeyManagerEvent::KeyChange, got different variant"),
            }
        }

        // 🔑 Governance Key sets the new Governance Key 🔑
        {
            let transaction_hash = H256::from_str(
                "0x9215ce54309fddf0ce9b1e8fd10319c62cf9603635ffa0c06ac9db8338348f95",
            )
            .unwrap();
            match decode_log(
                key_change_event_signature,
                transaction_hash,
                RawLog {
                    topics : vec![key_change_event_signature],
                    data : hex::decode("0000000000000000000000000000000000000000000000000000000000000000423ebe9d54bf7cb10dfebe2b323bb9a01bfede660619a7f49531c96a23263dd800000000000000000000000000000000000000000000000000000000000000014e3d72babbee4133675d42db3bba62a7dfbc47a91ddc5db56d95313d908c08f80000000000000000000000000000000000000000000000000000000000000000").unwrap()
                }
            ).expect("Failed parsing GOV_SET_GOV_LOG event")
            {
                KeyManagerEvent::KeyChange {
                    signed,
                    old_key,
                    new_key,
                    tx_hash,
                } => {
                    assert_eq!(signed, false);
                    assert_eq!(old_key, ChainflipKey::from_dec_str("29963508097954364125322164523090632495724997135004046323041274775773196467672",true).unwrap());
                    assert_eq!(new_key, ChainflipKey::from_dec_str("35388971693871284788334991319340319470612669764652701045908837459480931993848",false).unwrap());

                    assert_eq!(tx_hash, transaction_hash.to_fixed_bytes());
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
                H256::from_str("0x04629152b064c0d1343161c43f3b78cf67e9be35fc97f66bbb0e1ca1a0206bae").unwrap(),
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
        let transaction_hash =
            H256::from_str("0xae857f31e9543b0dd1e2092f049897045107e009c281ddf24d32dd5d80ec7492")
                .unwrap();

        match decode_log(
            refunded_event_signature,
            transaction_hash,
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
            KeyManagerEvent::Refunded { amount } => {
                assert_eq!(11126819398980, amount);
            }
            _ => panic!("Expected KeyManager::Refunded, got a different variant"),
        }
    }
}
