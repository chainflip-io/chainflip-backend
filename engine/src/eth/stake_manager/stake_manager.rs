//! Contains the information required to use the StakeManger contract as a source for
//! the EthEventStreamer

use core::str::FromStr;
use std::{convert::TryInto, fmt::Display};

use crate::{
    eth::{utils, EventProducerError, EventSource},
    logging::COMPONENT_KEY,
};

use serde::{Deserialize, Serialize};
use slog::o;
use sp_runtime::AccountId32;
use web3::{
    ethabi::{self, Function, Log},
    types::{BlockNumber, FilterBuilder, H160},
};

use anyhow::Result;

#[derive(Clone)]
/// A wrapper for the StakeManager Ethereum contract.
pub struct StakeManager {
    pub deployed_address: H160,
    contract: ethabi::Contract,
    logger: slog::Logger,
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
    pub fn load(deployed_address: &str, logger: &slog::Logger) -> Result<Self> {
        slog::info!(
            logger,
            "Loading in stake manager contract abi. Connecting to contract at: {}",
            deployed_address
        );
        let abi_bytes = std::include_bytes!("../abis/StakeManager.json");
        let contract = ethabi::Contract::load(abi_bytes.as_ref())?;

        Ok(Self {
            deployed_address: H160::from_str(deployed_address)?,
            contract,
            logger: logger.new(o!(COMPONENT_KEY => "StakeManager")),
        })
    }

    /// Event definition for the 'Staked' event
    pub fn staked_event_definition(&self) -> &ethabi::Event {
        self.get_event("Staked")
            .expect("StakeManager contract should provide 'Staked' event.")
    }

    /// Event definition for the 'ClaimRegistered' event
    pub fn claim_registered_event_definition(&self) -> &ethabi::Event {
        self.get_event("ClaimRegistered")
            .expect("StakeManager contract should provide 'ClaimRegistered' event")
    }

    /// Event definition for the 'ClaimExecuted' event
    pub fn claim_executed_event_definition(&self) -> &ethabi::Event {
        self.get_event("ClaimExecuted")
            .expect("StakeManager contract should provide 'ClaimExecuted' event.")
    }

    /// Event definition for the 'FlipSupplyUpdated' event
    pub fn emission_changed_event_definition(&self) -> &ethabi::Event {
        self.get_event("FlipSupplyUpdated")
            .expect("StakeManager contract should provide 'FlipSupplyUpdated' event")
    }

    /// Event definition for the 'MinStakeChanged' event
    pub fn min_stake_changed_event_definition(&self) -> &ethabi::Event {
        self.get_event("MinStakeChanged")
            .expect("StakeManager contract should provide 'MinStakeChanged' event")
    }

    // Get the event type definition from the contract abi
    fn get_event(&self, name: &str) -> Result<&ethabi::Event> {
        Ok(self.contract.event(name)?)
    }

    /// Extracts a reference to the "registerClaim" function definition. Panics if it can't be found.
    pub fn register_claim(&self) -> &Function {
        self.contract
            .function("registerClaim")
            .expect("Function 'register_claim' should be defined in the StakeManager abi.")
    }
}

// get the node_id from the log and return as AccountId32
fn node_id_from_log(log: &Log) -> Result<AccountId32> {
    let account_bytes: [u8; 32] = utils::decode_log_param::<ethabi::FixedBytes>(&log, "nodeID")?
        .try_into()
        .map_err(|_| anyhow::Error::msg("Could not cast FixedBytes nodeID into [u8;32]"))?;
    Ok(AccountId32::new(account_bytes))
}

impl EventSource for StakeManager {
    type Event = StakeManagerEvent;

    fn filter_builder(&self, block: BlockNumber) -> FilterBuilder {
        FilterBuilder::default()
            .from_block(block)
            .address(vec![self.deployed_address])
    }

    fn parse_event(&self, log: web3::types::Log) -> Result<Self::Event> {
        let sig = log
            .topics
            .first()
            .ok_or_else(|| EventProducerError::EmptyTopics)?
            .clone();

        let tx_hash = log
            .transaction_hash
            .ok_or(anyhow::Error::msg(
                "Could not get transaction hash from ETH log",
            ))?
            .to_fixed_bytes();

        let raw_log = ethabi::RawLog {
            topics: log.topics,
            data: log.data.0,
        };

        slog::debug!(
            self.logger,
            "Parsing event from block {:?} with signature: {:?}",
            log.block_number.unwrap_or_default(),
            sig
        );

        match sig {
            _ if sig == self.staked_event_definition().signature() => {
                let log = self.staked_event_definition().parse_log(raw_log)?;
                let account_id = node_id_from_log(&log)?;
                let event = StakeManagerEvent::Staked {
                    account_id,
                    amount: utils::decode_log_param::<ethabi::Uint>(&log, "amount")?.as_u128(),
                    return_addr: utils::decode_log_param(&log, "returnAddr")?,
                    tx_hash,
                };
                Ok(event)
            }
            _ if sig == self.claim_executed_event_definition().signature() => {
                let log = self.claim_executed_event_definition().parse_log(raw_log)?;
                let account_id = node_id_from_log(&log)?;
                let event = StakeManagerEvent::ClaimExecuted {
                    account_id,
                    amount: utils::decode_log_param::<ethabi::Uint>(&log, "amount")?.as_u128(),
                    tx_hash,
                };
                Ok(event)
            }
            // The rest of the events are left in ethabi form, not required by other components (yet)
            _ if sig == self.emission_changed_event_definition().signature() => {
                let log = self
                    .emission_changed_event_definition()
                    .parse_log(raw_log)?;
                let event = StakeManagerEvent::FlipSupplyUpdated {
                    old_supply: utils::decode_log_param(&log, "oldSupply")?,
                    new_supply: utils::decode_log_param(&log, "newSupply")?,
                    block_number: utils::decode_log_param(&log, "stateChainBlockNumber")?,
                    tx_hash,
                };
                Ok(event)
            }
            _ if sig == self.min_stake_changed_event_definition().signature() => {
                let log = self
                    .min_stake_changed_event_definition()
                    .parse_log(raw_log)?;
                let event = StakeManagerEvent::MinStakeChanged {
                    old_min_stake: utils::decode_log_param(&log, "oldMinStake")?,
                    new_min_stake: utils::decode_log_param(&log, "newMinStake")?,
                    tx_hash,
                };
                Ok(event)
            }
            _ if sig == self.claim_registered_event_definition().signature() => {
                let log = self
                    .claim_registered_event_definition()
                    .parse_log(raw_log)?;
                let account_id = node_id_from_log(&log)?;
                let event = StakeManagerEvent::ClaimRegistered {
                    account_id,
                    amount: utils::decode_log_param(&log, "amount")?,
                    staker: utils::decode_log_param(&log, "staker")?,
                    start_time: utils::decode_log_param(&log, "startTime")?,
                    expiry_time: utils::decode_log_param(&log, "expiryTime")?,
                    tx_hash,
                };
                Ok(event)
            }
            s => Err(EventProducerError::UnexpectedEvent(s))?,
        }
    }
}

#[cfg(test)]
mod tests {

    use web3::types::{H256, U256};

    use crate::logging;

    use super::*;

    const CONTRACT_ADDRESS: &'static str = "0xEAd5De9C41543E4bAbB09f9fE4f79153c036044f";

    const STAKED_EVENT_SIG: &'static str =
        "0x23581b9afdc2170a53868d0b64508f096844aa55c3ad98caf14032a91c41cc52";

    const CLAIM_REGISTERED_EVENT_SIG: &'static str =
        "0x2f73775f2573d45f5b0ed0064eb65f631ac9e568a52807221c44ca9d358a9cee";

    const CLAIM_EXECUTED_EVENT_SIG: &'static str =
        "0xac96f597a44ad425c6eedf6e4c8327fd959c9d912fa8d027fb54313e59f247c8";

    const FLIP_SUPPLY_UPDATED_EVENT_SIG: &'static str =
        "0xff4b7a826623672c6944dc44d809008e2e1105180d110fd63986e841f15eb2ad";

    const MIN_STAKE_CHANGED_EVENT_SIG: &'static str =
        "0xca11c8a4c461b60c9f485404c272650c2aaae260b2067d72e9924abb68556593";

    const STAKED_LOG: &'static str = r#"{
        "logIndex": "0x2", 
        "transactionIndex": "0x0",
        "transactionHash": "0x9158e6d1470330d9d38636930831d5ee17fb71af70f3f17794539d50e00b08aa", 
        "blockHash": "0x17c2c0ca7b4ff256e6bcec927535a081bc0d6274523abee01f02daed24e9a3ab", 
        "blockNumber": "0xa", 
        "address": "0x6951b5Bd815043E3F842c1b026b0Fa888Cc2DD85", 
        "data": "0x000000000000000000000000000000000000000000000878678326eac90000000000000000000000000000000000000000000000000000000000000000000001", 
        "topics": [
            "0x23581b9afdc2170a53868d0b64508f096844aa55c3ad98caf14032a91c41cc52",
            "0x0000000000000000000000000000000000000000000000000000000000003039"
        ],
        "type": "mined",
        "removed": false
        
    }"#;

    const CLAIM_REGISTERED_LOG: &'static str = r#"{
        "address": "0x85c0d660ea89da58c05996eb8fb7a444b3543f11",
        "blockHash": "0xa634f3f33c765cd850b8972d539db5b4c6385d862a89a8d15fdbc25db3eec19a",
        "blockNumber": "0x8669f6",
        "data": "0x0000000000000000000000000000000000000000000002d2cd2bb7a39860000000000000000000000000000073d669c173d88ccb01f6daab3a3304af7a1b22c10000000000000000000000000000000000000000000000000000000060d4910f0000000000000000000000000000000000000000000000000000000060d73402",
        "logIndex": "0x5",
        "removed": false,
        "topics": [
            "0x2f73775f2573d45f5b0ed0064eb65f631ac9e568a52807221c44ca9d358a9cee",
            "0x0000000000000000000000000000000000000000000000000000000000003039"
        ],
        "transactionHash": "0x4e3f3296f3baff3763bd2beb9cdfa6ddeb996c409f746f0450093712f2417185",
        "transactionIndex": "0x14"
    }"#;

    const CLAIM_EXECUTED_LOG: &'static str = r#"{
        "address": "0xe0fb3e945afacbd5f604eba6dbe9be4486d1926b",
        "blockHash": "0x491d175abcae9fb7a96266614d4494f8cec3a5f77092d4a4b4992de816354544",
        "blockNumber": "0x867c7a",
        "data": "0x0000000000000000000000000000000000000000000002d2cd2bb7a398600000",
        "logIndex": "0x19",
        "removed": false,
        "topics": [
            "0xac96f597a44ad425c6eedf6e4c8327fd959c9d912fa8d027fb54313e59f247c8",
            "0x0000000000000000000000000000000000000000000000000000000000003039"
        ],
        "transactionHash": "0x99264107b21be2fb9beb1e4e8d47dc431df6696651f1937ece635a7960849605",
        "transactionIndex": "0xe"
    }"#;

    const FLIP_SUPPLY_UPDATED_LOG: &'static str = r#"{
        "logIndex": "0x2", 
        "transactionIndex": "0x0", 
        "transactionHash": "0x06a6ef6fb6ab3a9493435d37a36607efc197dc71518b68b25d1061116034b16f", 
        "blockHash": "0x6518112e02b160ef699990355d752dbf402a19f472ea18e6bdd575e0a3351c1a", 
        "blockNumber": "0xe", 
        "address": "0x6951b5Bd815043E3F842c1b026b0Fa888Cc2DD85", 
        "data": "0x0000000000000000000000000000000000000000004a723dc6b40b8a9a00000000000000000000000000000000000000000000000052b7d2dcc80cd2e40000000000000000000000000000000000000000000000000000000000000000000064", 
        "topics": ["0xff4b7a826623672c6944dc44d809008e2e1105180d110fd63986e841f15eb2ad"], 
        "type": "mined",
        "removed": false
    }"#;

    const MIN_STAKE_CHANGED_LOG: &'static str = r#"{
        "address": "0x85c0d660ea89da58c05996eb8fb7a444b3543f11",
        "blockHash": "0x52cf218d1a7fccc2ba91ce79be681d49f0532c4b2f987d98578b6041e7c4b057",
        "blockNumber": "0x8669f7",
        "data": "0x000000000000000000000000000000000000000000000878678326eac90000000000000000000000000000000000000000000000000002d2cd2bb7a398600000",
        "logIndex": "0x1b",
        "removed": false,
        "topics": [
            "0xca11c8a4c461b60c9f485404c272650c2aaae260b2067d72e9924abb68556593"
        ],
        "transactionHash": "0x7224ca5aae97dc9f9b25fd0ba337fd936709d277cf8600786c4168e1d86d7c1f",
        "transactionIndex": "0x1b"
    }"#;

    #[test]
    fn test_load_contract() {
        let logger = logging::test_utils::create_test_logger();
        assert!(StakeManager::load(CONTRACT_ADDRESS, &logger).is_ok());
        assert!(StakeManager::load("not_an_address", &logger).is_err());
    }

    #[test]
    fn test_staked_log_parsing() {
        let log: web3::types::Log = serde_json::from_str(STAKED_LOG).unwrap();

        let sm = StakeManager::load(CONTRACT_ADDRESS, &logging::test_utils::create_test_logger())
            .unwrap();

        match sm.parse_event(log).unwrap() {
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
                let expected_hash = H256::from_str(
                    "0x9158e6d1470330d9d38636930831d5ee17fb71af70f3f17794539d50e00b08aa",
                )
                .unwrap()
                .to_fixed_bytes();
                assert_eq!(
                    return_addr,
                    web3::types::H160::from_str("0x0000000000000000000000000000000000000001")
                        .unwrap()
                );
                assert_eq!(tx_hash, expected_hash);
            }
            _ => panic!("Expected StakeManagerEvent::Staked, got a different variant"),
        }
    }

    #[test]
    fn test_claim_registered_log_parsing() {
        let log: web3::types::Log = serde_json::from_str(CLAIM_REGISTERED_LOG).unwrap();

        let sm = StakeManager::load(CONTRACT_ADDRESS, &logging::test_utils::create_test_logger())
            .unwrap();

        match sm.parse_event(log).unwrap() {
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
                let expected_hash = H256::from_str(
                    "0x4e3f3296f3baff3763bd2beb9cdfa6ddeb996c409f746f0450093712f2417185",
                )
                .unwrap()
                .to_fixed_bytes();
                assert_eq!(tx_hash, expected_hash)
            }
            _ => panic!("Expected Staking::ClaimRegistered, got a different variant"),
        }
    }

    #[test]
    fn test_claim_executed_log_parsing() {
        let log: web3::types::Log = serde_json::from_str(CLAIM_EXECUTED_LOG).unwrap();

        let sm = StakeManager::load(CONTRACT_ADDRESS, &logging::test_utils::create_test_logger())
            .unwrap();

        match sm.parse_event(log).unwrap() {
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
                let expected_hash = H256::from_str(
                    "0x99264107b21be2fb9beb1e4e8d47dc431df6696651f1937ece635a7960849605",
                )
                .unwrap()
                .to_fixed_bytes();
                assert_eq!(tx_hash, expected_hash);
            }
            _ => panic!("Expected Staking::ClaimExecuted, got a different variant"),
        }
    }

    #[test]
    fn flip_supply_updated_log_parsing() {
        let log: web3::types::Log = serde_json::from_str(FLIP_SUPPLY_UPDATED_LOG).unwrap();

        let sm = StakeManager::load(CONTRACT_ADDRESS, &logging::test_utils::create_test_logger())
            .unwrap();

        match sm.parse_event(log).unwrap() {
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
                let expected_hash = H256::from_str(
                    "0x06a6ef6fb6ab3a9493435d37a36607efc197dc71518b68b25d1061116034b16f",
                )
                .unwrap()
                .to_fixed_bytes();
                assert_eq!(tx_hash, expected_hash);
            }
            _ => panic!("Expected Staking::FlipSupplyUpdated, got a different variant"),
        }
    }

    #[test]
    fn min_stake_changed_log_parsing() {
        let log: web3::types::Log = serde_json::from_str(MIN_STAKE_CHANGED_LOG).unwrap();

        let sm = StakeManager::load(CONTRACT_ADDRESS, &logging::test_utils::create_test_logger())
            .unwrap();

        match sm.parse_event(log).unwrap() {
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

                let expected_hash = H256::from_str(
                    "0x7224ca5aae97dc9f9b25fd0ba337fd936709d277cf8600786c4168e1d86d7c1f",
                )
                .unwrap()
                .to_fixed_bytes();
                assert_eq!(tx_hash, expected_hash);
            }
            _ => panic!("Expected Staking::MinStakeChanged, got a different variant"),
        }
    }

    #[test]
    fn abi_topic_sigs() {
        let sm = StakeManager::load(CONTRACT_ADDRESS, &logging::test_utils::create_test_logger())
            .unwrap();

        // Staked event
        let staked_sig = sm.staked_event_definition().signature();
        let expected =
            H256::from_str(STAKED_EVENT_SIG).expect("Couldn't cast staked event sig to H256");
        assert_eq!(staked_sig, expected, "Staked event doesn't match signature");

        // ClaimRegistered event
        let claim_registered_sig = sm.claim_registered_event_definition().signature();
        let expected = H256::from_str(CLAIM_REGISTERED_EVENT_SIG)
            .expect("Couldn't cast claim_registered sig to H256");
        assert_eq!(claim_registered_sig, expected);

        // Claimed event
        let claim_executed_sig = sm.claim_executed_event_definition().signature();
        let expected = H256::from_str(CLAIM_EXECUTED_EVENT_SIG)
            .expect("Couldn't cast claimed event sig to H256");
        assert_eq!(claim_executed_sig, expected);

        // Emission changed event
        let emission_changed_sig = sm.emission_changed_event_definition().signature();
        let expected = H256::from_str(FLIP_SUPPLY_UPDATED_EVENT_SIG)
            .expect("Couldn't cast emission changed event sig to H256");
        assert_eq!(emission_changed_sig, expected);

        // Min stake changed
        let min_stake_changed_sig = sm.min_stake_changed_event_definition().signature();
        let expected = H256::from_str(MIN_STAKE_CHANGED_EVENT_SIG)
            .expect("Couldn't case min stake changed event sig to H256");
        assert_eq!(min_stake_changed_sig, expected);
    }
}
