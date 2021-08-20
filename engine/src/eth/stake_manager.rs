//! Contains the information required to use the StakeManger contract as a source for
//! the EthEventStreamer

use core::str::FromStr;
use std::{
    convert::TryInto,
    fmt::Display,
    sync::{Arc, Mutex},
};

use crate::{
    eth::{eth_event_streamer, utils, EventParseError, SignatureAndEvent},
    logging::COMPONENT_KEY,
    settings,
    state_chain::pallets::witness_api::*,
    state_chain::runtime::StateChainRuntime,
};

use serde::{Deserialize, Serialize};
use sp_runtime::AccountId32;
use substrate_subxt::{Client, PairSigner};

use web3::{
    ethabi::{self, Log, RawLog},
    transports::WebSocket,
    types::{H160, H256},
    Web3,
};

use anyhow::Result;

use futures::{Future, StreamExt};
use slog::o;

/// Set up the eth event streamer for the StakeManager contract, and start it
pub async fn start_stake_manager_witness(
    web3: &Web3<WebSocket>,
    settings: &settings::Settings,
    signer: Arc<Mutex<PairSigner<StateChainRuntime, sp_core::sr25519::Pair>>>,
    subxt_client: Client<StateChainRuntime>,
    logger: &slog::Logger,
) -> Result<impl Future> {
    let logger = logger.new(o!(COMPONENT_KEY => "StakeManagerWitness"));

    slog::info!(logger, "Starting StakeManager witness");

    let stake_manager = StakeManager::new(&settings)?;

    let mut event_stream = eth_event_streamer::new_eth_event_stream(
        web3.clone(),
        stake_manager.deployed_address,
        settings.eth.from_block,
        logger.clone(),
    )
    .await?;

    let parser = stake_manager.parser_closure()?;

    Ok(async move {
        while let Some(result_event) = event_stream.next().await {
            async {
                match parser(result_event.unwrap()).unwrap() {
                    // TODO: Handle unwraps
                    StakeManagerEvent::Staked {
                        account_id,
                        amount,
                        return_addr,
                        tx_hash,
                    } => {
                        slog::trace!(
                            logger,
                            "Sending witness_staked({:?}, {}, {:?}, {:?}) to state chain",
                            account_id,
                            amount,
                            return_addr,
                            tx_hash
                        );
                        let mut signer = signer.lock().unwrap();
                        subxt_client
                            .witness_staked(&*signer, account_id, amount, tx_hash)
                            .await?;
                        signer.increment_nonce();
                    }
                    StakeManagerEvent::ClaimExecuted {
                        account_id,
                        amount,
                        tx_hash,
                    } => {
                        slog::trace!(
                            logger,
                            "Sending claim_executed({:?}, {}, {:?}) to the state chain",
                            account_id,
                            amount,
                            tx_hash
                        );
                        let mut signer = signer.lock().unwrap();
                        subxt_client
                            .witness_claimed(&*signer, account_id, amount, tx_hash)
                            .await?;
                        signer.increment_nonce();
                    }
                    event => {
                        slog::warn!(
                            logger,
                            "{} is not to be submitted to the State Chain",
                            event
                        );
                    }
                }
                Result::<(), anyhow::Error>::Ok(())
            }
            .await
            .unwrap(); // TODO: How to handle call errors
        }
    })
}

#[derive(Clone)]
/// A wrapper for the StakeManager Ethereum contract.
pub struct StakeManager {
    pub deployed_address: H160,
    contract: ethabi::Contract,
}

// TODO: ClaimRegistered, FlipSupplyUpdated, MinStakeChanged, not used
// so they are just using the ethabi encoding atm
/// Represents the events that are expected from the StakeManager contract.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StakeManagerEvent {
    /// The `Staked(nodeId, amount)` event.
    Staked {
        /// The node id of the validator that submitted the stake.
        account_id: AccountId32,
        /// The amount of FLIP that was staked.
        amount: u128,
        /// The address which the staker requires to be used when claiming back FLIP for `nodeID`
        return_addr: ethabi::Address,
        /// Transaction hash that created the event
        tx_hash: [u8; 32],
    },

    /// `ClaimRegistered(nodeId, amount, staker, startTime, expiryTime)` event
    ClaimRegistered {
        /// Node id of the validator registering the claim
        account_id: AccountId32,
        /// Amount the validator is claiming
        amount: ethabi::Uint,
        /// The ETH address of the validator, used to stake their FLIP
        staker: ethabi::Address,
        /// The start time of the claim
        start_time: ethabi::Uint,
        /// The expiry time of the claim
        expiry_time: ethabi::Uint,
        /// Transaction hash that created the event
        tx_hash: [u8; 32],
    },

    /// `ClaimExecuted(nodeId, amount)` event
    ClaimExecuted {
        /// The node id of the validator that claimed their FLIP
        account_id: AccountId32,
        /// The amount of FLIP that was claimed
        amount: u128,
        /// Transaction hash that created the event
        tx_hash: [u8; 32],
    },

    /// `FlipSupplyUpdated(oldSupply, newTotalSupply, stateChainBlockNumber)` event
    FlipSupplyUpdated {
        /// Old emission per block
        old_supply: ethabi::Uint,
        /// New emission per block
        new_supply: ethabi::Uint,
        /// State Chain block number for the new total supply
        block_number: ethabi::Uint,
        /// Transaction hash that created the event
        tx_hash: [u8; 32],
    },

    /// `MinStakeChanged(oldMinStake, newMinStake)`
    MinStakeChanged {
        /// Old minimum stake
        old_min_stake: ethabi::Uint,
        /// New minimum stake
        new_min_stake: ethabi::Uint,
        /// Transaction hash that created the event
        tx_hash: [u8; 32],
    },
}

impl Display for StakeManagerEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            StakeManagerEvent::Staked {
                account_id,
                amount,
                return_addr,
                tx_hash,
            } => write!(
                f,
                "Staked({:?}, {}, {:?}, {:?}",
                account_id, amount, return_addr, tx_hash
            ),
            StakeManagerEvent::ClaimRegistered {
                account_id,
                amount,
                staker,
                start_time,
                expiry_time,
                tx_hash,
            } => write!(
                f,
                "ClaimRegistered({:?}, {}, {}, {}, {}, {:?}",
                account_id, amount, staker, start_time, expiry_time, tx_hash
            ),
            StakeManagerEvent::ClaimExecuted {
                account_id,
                amount,
                tx_hash,
            } => {
                write!(
                    f,
                    "ClaimExecuted({:?}, {}, {:?}",
                    account_id, amount, tx_hash
                )
            }
            StakeManagerEvent::FlipSupplyUpdated {
                old_supply,
                new_supply,
                block_number,
                tx_hash,
            } => write!(
                f,
                "FlipSupplyUpdated({}, {}, {}, {:?}",
                old_supply, new_supply, block_number, tx_hash
            ),
            StakeManagerEvent::MinStakeChanged {
                old_min_stake,
                new_min_stake,
                tx_hash,
            } => write!(
                f,
                "MinStakeChanged({}, {}, {:?}",
                old_min_stake, new_min_stake, tx_hash
            ),
        }
    }
}

impl StakeManager {
    /// Loads the contract abi to get event definitions
    pub fn new(settings: &settings::Settings) -> Result<Self> {
        let contract =
            ethabi::Contract::load(std::include_bytes!("abis/StakeManager.json").as_ref())?;
        Ok(Self {
            deployed_address: H160::from_str(&settings.eth.stake_manager_eth_address)?,
            contract,
        })
    }

    // get the node_id from the log and return as AccountId32
    fn node_id_from_log(log: &Log) -> Result<AccountId32> {
        let account_bytes: [u8; 32] =
            utils::decode_log_param::<ethabi::FixedBytes>(&log, "nodeID")?
                .try_into()
                .map_err(|_| anyhow::Error::msg("Could not cast FixedBytes nodeID into [u8;32]"))?;
        Ok(AccountId32::new(account_bytes))
    }

    pub fn parser_closure(
        &self,
    ) -> Result<impl Fn((H256, H256, ethabi::RawLog)) -> Result<StakeManagerEvent>> {
        let staked = SignatureAndEvent::new(&self.contract, "Staked")?;
        let claim_registered = SignatureAndEvent::new(&self.contract, "ClaimRegistered")?;
        let claim_executed = SignatureAndEvent::new(&self.contract, "ClaimExecuted")?;
        let flip_supply_updated = SignatureAndEvent::new(&self.contract, "FlipSupplyUpdated")?;
        let min_stake_changed = SignatureAndEvent::new(&self.contract, "MinStakeChanged")?;

        Ok(move |(signature, tx_hash, raw_log) : (H256, H256, RawLog)| -> Result<StakeManagerEvent> {
            let tx_hash = tx_hash.to_fixed_bytes();
            if signature == staked.signature {
                let log = staked.event.parse_log(raw_log)?;
                let account_id = Self::node_id_from_log(&log)?;
                let event = StakeManagerEvent::Staked {
                    account_id,
                    amount: utils::decode_log_param::<ethabi::Uint>(&log, "amount")?.as_u128(),
                    return_addr: utils::decode_log_param(&log, "returnAddr")?,
                    tx_hash,
                };
                Ok(event)
            } else if signature == claim_registered.signature {
                let log = claim_registered.event.parse_log(raw_log)?;
                let account_id = Self::node_id_from_log(&log)?;
                let event = StakeManagerEvent::ClaimRegistered {
                    account_id,
                    amount: utils::decode_log_param(&log, "amount")?,
                    staker: utils::decode_log_param(&log, "staker")?,
                    start_time: utils::decode_log_param(&log, "startTime")?,
                    expiry_time: utils::decode_log_param(&log, "expiryTime")?,
                    tx_hash,
                };
                Ok(event)
            } else if signature == claim_executed.signature {
                let log = claim_executed.event.parse_log(raw_log)?;
                let account_id = Self::node_id_from_log(&log)?;
                let event = StakeManagerEvent::ClaimExecuted {
                    account_id,
                    amount: utils::decode_log_param::<ethabi::Uint>(&log, "amount")?.as_u128(),
                    tx_hash,
                };
                Ok(event)
            } else if signature == flip_supply_updated.signature {
                let log = flip_supply_updated.event.parse_log(raw_log)?;
                let event = StakeManagerEvent::FlipSupplyUpdated {
                    old_supply: utils::decode_log_param(&log, "oldSupply")?,
                    new_supply: utils::decode_log_param(&log, "newSupply")?,
                    block_number: utils::decode_log_param(&log, "stateChainBlockNumber")?,
                    tx_hash,
                };
                Ok(event)
            } else if signature == min_stake_changed.signature {
                let log = min_stake_changed.event.parse_log(raw_log)?;
                let event = StakeManagerEvent::MinStakeChanged {
                    old_min_stake: utils::decode_log_param(&log, "oldMinStake")?,
                    new_min_stake: utils::decode_log_param(&log, "newMinStake")?,
                    tx_hash,
                };
                Ok(event)
            } else {
                Err(anyhow::Error::from(EventParseError::UnexpectedEvent(signature)))
            }
        })
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use hex;
    use web3::types::{H256, U256};

    #[test]
    fn test_load_contract() {
        let mut settings = settings::test_utils::new_test_settings().unwrap();
        assert!(StakeManager::new(&settings).is_ok());
        settings.eth.stake_manager_eth_address = "not_an_address".to_string();
        assert!(StakeManager::new(&settings).is_err());
    }

    #[test]
    fn test_staked_log_parsing() {
        let settings = settings::test_utils::new_test_settings().unwrap();

        let stake_manager = StakeManager::new(&settings).unwrap();
        let parser = stake_manager.parser_closure().unwrap();

        let staked_event_signature =
            H256::from_str("0x23581b9afdc2170a53868d0b64508f096844aa55c3ad98caf14032a91c41cc52")
                .unwrap();
        let transaction_hash =
            H256::from_str("0x9158e6d1470330d9d38636930831d5ee17fb71af70f3f17794539d50e00b08aa")
                .unwrap();
        match parser((
            staked_event_signature,
            transaction_hash,
            RawLog {
                topics : vec![
                    staked_event_signature,
                    H256::from_str("0x0000000000000000000000000000000000000000000000000000000000003039").unwrap()
                ],
                data : hex::decode("000000000000000000000000000000000000000000000878678326eac90000000000000000000000000000000000000000000000000000000000000000000001").unwrap()
            }
        )).unwrap() {
            StakeManagerEvent::Staked {
                account_id,
                amount,
                return_addr,
                tx_hash,
            } => {
                let expected_account_id =
                    AccountId32::from_str("5C4hrfjw9DjXZTzV3MwzrrAr9P1MJhSrvWGWqi1eSuziKFgU")
                        .unwrap();
                assert_eq!(account_id, expected_account_id);
                assert_eq!(amount, 40000000000000000000000u128);
                assert_eq!(
                    return_addr,
                    web3::types::H160::from_str("0x0000000000000000000000000000000000000001")
                        .unwrap()
                );
                assert_eq!(tx_hash, transaction_hash.to_fixed_bytes());
            }
            _ => panic!("Expected StakeManagerEvent::Staked, got a different variant"),
        }
    }

    #[test]
    fn test_claim_registered_log_parsing() {
        let settings = settings::test_utils::new_test_settings().unwrap();

        let stake_manager = StakeManager::new(&settings).unwrap();
        let parser = stake_manager.parser_closure().unwrap();

        let claimed_register_event_signature =
            H256::from_str("0x2f73775f2573d45f5b0ed0064eb65f631ac9e568a52807221c44ca9d358a9cee")
                .unwrap();
        let transaction_hash =
            H256::from_str("0x4e3f3296f3baff3763bd2beb9cdfa6ddeb996c409f746f0450093712f2417185")
                .unwrap();
        match parser((
            claimed_register_event_signature,
            transaction_hash,
            RawLog {
                topics : vec![
                    claimed_register_event_signature,
                    H256::from_str("0x0000000000000000000000000000000000000000000000000000000000003039").unwrap()
                ],
                data : hex::decode("0000000000000000000000000000000000000000000002d2cd2bb7a39860000000000000000000000000000073d669c173d88ccb01f6daab3a3304af7a1b22c10000000000000000000000000000000000000000000000000000000060d4910f0000000000000000000000000000000000000000000000000000000060d73402").unwrap()
            }
        )).unwrap() {
            StakeManagerEvent::ClaimRegistered {
                account_id,
                amount,
                staker,
                start_time,
                expiry_time,
                tx_hash,
            } => {
                assert_eq!(
                    account_id,
                    AccountId32::from_str("5C4hrfjw9DjXZTzV3MwzrrAr9P1MJhSrvWGWqi1eSuziKFgU")
                        .unwrap()
                );
                assert_eq!(
                    amount,
                    web3::types::U256::from_dec_str("13333333333333334032384").unwrap()
                );
                assert_eq!(
                    staker,
                    web3::types::H160::from_str("0x73d669c173d88ccb01f6daab3a3304af7a1b22c1")
                        .unwrap()
                );
                assert_eq!(
                    start_time,
                    web3::types::U256::from_dec_str("1624543503").unwrap()
                );
                assert_eq!(
                    expiry_time,
                    web3::types::U256::from_dec_str("1624716290").unwrap()
                );
                assert_eq!(tx_hash, transaction_hash.to_fixed_bytes())
            }
            _ => panic!("Expected Staking::ClaimRegistered, got a different variant"),
        }
    }

    #[test]
    fn test_claim_executed_log_parsing() {
        let settings = settings::test_utils::new_test_settings().unwrap();

        let stake_manager = StakeManager::new(&settings).unwrap();
        let parser = stake_manager.parser_closure().unwrap();

        let claimed_executed_event_signature =
            H256::from_str("0xac96f597a44ad425c6eedf6e4c8327fd959c9d912fa8d027fb54313e59f247c8")
                .unwrap();
        let transaction_hash =
            H256::from_str("0x99264107b21be2fb9beb1e4e8d47dc431df6696651f1937ece635a7960849605")
                .unwrap();
        match parser((
            claimed_executed_event_signature,
            transaction_hash,
            RawLog {
                topics: vec![
                    claimed_executed_event_signature,
                    H256::from_str(
                        "0x0000000000000000000000000000000000000000000000000000000000003039",
                    )
                    .unwrap(),
                ],
                data: hex::decode(
                    "0000000000000000000000000000000000000000000002d2cd2bb7a398600000",
                )
                .unwrap(),
            },
        ))
        .unwrap()
        {
            StakeManagerEvent::ClaimExecuted {
                account_id,
                amount,
                tx_hash,
            } => {
                let expected_node_id =
                    AccountId32::from_str("5C4hrfjw9DjXZTzV3MwzrrAr9P1MJhSrvWGWqi1eSuziKFgU")
                        .unwrap();
                assert_eq!(account_id, expected_node_id);
                assert_eq!(amount, 13333333333333334032384);
                assert_eq!(tx_hash, transaction_hash.to_fixed_bytes());
            }
            _ => panic!("Expected Staking::ClaimExecuted, got a different variant"),
        }
    }

    #[test]
    fn flip_supply_updated_log_parsing() {
        let settings = settings::test_utils::new_test_settings().unwrap();

        let stake_manager = StakeManager::new(&settings).unwrap();
        let parser = stake_manager.parser_closure().unwrap();

        let flip_supply_updated_event_signature =
            H256::from_str("0xff4b7a826623672c6944dc44d809008e2e1105180d110fd63986e841f15eb2ad")
                .unwrap();
        let transaction_hash =
            H256::from_str("0x06a6ef6fb6ab3a9493435d37a36607efc197dc71518b68b25d1061116034b16f")
                .unwrap();
        match parser((
            flip_supply_updated_event_signature,
            transaction_hash,
            RawLog {
                topics : vec![flip_supply_updated_event_signature],
                data : hex::decode("0000000000000000000000000000000000000000004a723dc6b40b8a9a00000000000000000000000000000000000000000000000052b7d2dcc80cd2e40000000000000000000000000000000000000000000000000000000000000000000064").unwrap()
            }
        )).unwrap() {
            StakeManagerEvent::FlipSupplyUpdated {
                old_supply,
                new_supply,
                block_number,
                tx_hash,
            } => {
                assert_eq!(
                    old_supply,
                    U256::from_dec_str("90000000000000000000000000").unwrap()
                );
                assert_eq!(
                    new_supply,
                    U256::from_dec_str("100000000000000000000000000").unwrap()
                );
                assert_eq!(block_number, U256::from_dec_str("100").unwrap());
                assert_eq!(tx_hash, transaction_hash.to_fixed_bytes());
            }
            _ => panic!("Expected Staking::FlipSupplyUpdated, got a different variant"),
        }
    }

    #[test]
    fn min_stake_changed_log_parsing() {
        let settings = settings::test_utils::new_test_settings().unwrap();

        let stake_manager = StakeManager::new(&settings).unwrap();
        let parser = stake_manager.parser_closure().unwrap();

        let min_stake_changed_event_signature =
            H256::from_str("0xca11c8a4c461b60c9f485404c272650c2aaae260b2067d72e9924abb68556593")
                .unwrap();
        let transaction_hash =
            H256::from_str("0x7224ca5aae97dc9f9b25fd0ba337fd936709d277cf8600786c4168e1d86d7c1f")
                .unwrap();
        match parser((
            min_stake_changed_event_signature,
            transaction_hash,
            RawLog {
                topics : vec![min_stake_changed_event_signature],
                data : hex::decode("000000000000000000000000000000000000000000000878678326eac90000000000000000000000000000000000000000000000000002d2cd2bb7a398600000").unwrap()
            }
        )).unwrap() {
            StakeManagerEvent::MinStakeChanged {
                old_min_stake,
                new_min_stake,
                tx_hash,
            } => {
                assert_eq!(
                    old_min_stake,
                    U256::from_dec_str("40000000000000000000000").unwrap()
                );
                assert_eq!(
                    new_min_stake,
                    U256::from_dec_str("13333333333333334032384").unwrap()
                );
                assert_eq!(tx_hash, transaction_hash.to_fixed_bytes());
            }
            _ => panic!("Expected Staking::MinStakeChanged, got a different variant"),
        }
    }
}
