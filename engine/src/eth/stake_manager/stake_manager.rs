//! Contains the information required to use the StakeManger contract as a source for
//! the EthEventStreamer

use core::str::FromStr;
use std::convert::TryInto;

use crate::eth::{EventProducerError, EventSource};

use serde::{Deserialize, Serialize};
use sp_runtime::AccountId32;
use web3::{
    contract::tokens::Tokenizable,
    ethabi::{self, ethereum_types, Log},
    types::{BlockNumber, FilterBuilder, H160},
};

use anyhow::Result;

/// A wrapper for the StakeManager Ethereum contract.
pub struct StakeManager {
    deployed_address: H160,
    contract: ethabi::Contract,
}

// TODO: ClaimRegistered, EmissionChanged, MinStakeChanged, not used
// so they are just using the ethabi encoding atm
/// Represents the events that are expected from the StakeManager contract.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StakeManagerEvent {
    /// The `Staked(nodeId, amount)` event.
    Staked(
        /// The node id of the validator that submitted the stake.
        AccountId32,
        /// The amount of FLIP that was staked.
        u128,
        /// Transaction hash that created the event
        [u8; 32],
    ),

    /// `ClaimRegistered(nodeId, amount, staker, startTime, expiryTime)` event
    ClaimRegistered(
        /// Node id of the validator registering the claim
        ethabi::Uint,
        /// Amount the validator is claiming
        ethabi::Uint,
        /// The ETH address of the validator, used to stake their FLIP
        ethabi::Address,
        /// The start time of the claim
        ethabi::Uint,
        /// The expiry time of the claim
        ethabi::Uint,
        /// Transaction hash that created the event
        [u8; 32],
    ),

    /// `ClaimExecuted(nodeId, amount)` event
    ClaimExecuted(
        /// The node id of the validator that claimed their FLIP
        AccountId32,
        /// The amount of FLIP that was claimed
        u128,
        /// Transaction hash that created the event
        [u8; 32],
    ),

    /// `EmissionChanged(oldEmissionPerBlock, newEmissionPerBlock)`
    EmissionChanged(
        /// Old emission per block
        ethabi::Uint,
        /// New emission per block
        ethabi::Uint,
        /// Transaction hash that created the event
        [u8; 32],
    ),

    /// `MinStakeChanged(oldMinStake, newMinStake)`
    MinStakeChanged(
        /// Old minimum stake
        ethabi::Uint,
        /// New minimum stake
        ethabi::Uint,
        /// Transaction hash that created the event
        [u8; 32],
    ),
}

impl StakeManager {
    /// Loads the contract abi to get event definitions
    pub fn load(deployed_address: &str) -> Result<Self> {
        let abi_bytes = std::include_bytes!("../abis/StakeManager.json");
        let contract = ethabi::Contract::load(abi_bytes.as_ref())?;

        Ok(Self {
            deployed_address: H160::from_str(deployed_address)?,
            contract,
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

    /// Event definition for the 'EmissionChanged' event
    pub fn emission_changed_event_definition(&self) -> &ethabi::Event {
        self.get_event("EmissionChanged")
            .expect("StakeManager contract should provide 'EmissionChanged' event")
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
            .expect("Log should contain a transaction hash");

        let tx_hash_bytes = tx_hash.to_fixed_bytes();

        let raw_log = ethabi::RawLog {
            topics: log.topics,
            data: log.data.0,
        };

        log::debug!(
            "Parsing event from block {:?} with signature: {:?}",
            log.block_number.unwrap_or_default(),
            sig
        );

        match sig {
            _ if sig == self.staked_event_definition().signature() => {
                let log = self.staked_event_definition().parse_log(raw_log)?;
                let account_bytes: [u8; 32] =
                    decode_log_param::<ethabi::FixedBytes>(&log, "nodeID")?
                        .try_into()
                        .expect("fuck");
                let account_id = AccountId32::new(account_bytes);
                let event = StakeManagerEvent::Staked(
                    account_id,
                    decode_log_param::<ethabi::Uint>(&log, "amount")?.as_u128(),
                    tx_hash_bytes,
                );
                Ok(event)
            }
            _ if sig == self.claim_executed_event_definition().signature() => {
                let log = self.claim_executed_event_definition().parse_log(raw_log)?;
                let account_bytes: [u8; 32] =
                    decode_log_param::<ethabi::FixedBytes>(&log, "nodeID")?
                        .try_into()
                        .expect("fuck");
                let account_id = AccountId32::new(account_bytes);
                let event = StakeManagerEvent::ClaimExecuted(
                    account_id,
                    decode_log_param::<ethabi::Uint>(&log, "amount")?.as_u128(),
                    tx_hash_bytes,
                );
                Ok(event)
            }
            // The rest of the events are left in ethabi form, not required by other components (yet)
            _ if sig == self.emission_changed_event_definition().signature() => {
                let log = self
                    .emission_changed_event_definition()
                    .parse_log(raw_log)?;
                let event = StakeManagerEvent::EmissionChanged(
                    decode_log_param(&log, "oldEmissionPerBlock")?,
                    decode_log_param(&log, "newEmissionPerBlock")?,
                    tx_hash_bytes,
                );
                Ok(event)
            }
            _ if sig == self.min_stake_changed_event_definition().signature() => {
                let log = self
                    .min_stake_changed_event_definition()
                    .parse_log(raw_log)?;
                let event = StakeManagerEvent::MinStakeChanged(
                    decode_log_param(&log, "oldMinStake")?,
                    decode_log_param(&log, "newMinStake")?,
                    tx_hash_bytes,
                );
                Ok(event)
            }
            _ if sig == self.claim_registered_event_definition().signature() => {
                let log = self
                    .claim_registered_event_definition()
                    .parse_log(raw_log)?;
                let event = StakeManagerEvent::ClaimRegistered(
                    decode_log_param(&log, "nodeID")?,
                    decode_log_param(&log, "amount")?,
                    decode_log_param(&log, "staker")?,
                    decode_log_param(&log, "startTime")?,
                    decode_log_param(&log, "expiryTime")?,
                    tx_hash_bytes,
                );
                Ok(event)
            }
            s => Err(EventProducerError::UnexpectedEvent(s))?,
        }
    }
}

// Helper method to decode the parameters from an ETH log
fn decode_log_param<T: Tokenizable>(log: &Log, param_name: &str) -> Result<T> {
    let token = &log
        .params
        .iter()
        .find(|&p| p.name == param_name)
        .ok_or_else(|| EventProducerError::MissingParam(String::from(param_name)))?
        .value;

    Ok(Tokenizable::from_token(token.clone())?)
}

#[cfg(test)]
mod tests {

    use web3::types::{H256, U256};

    use super::*;

    const CONTRACT_ADDRESS: &'static str = "0xEAd5De9C41543E4bAbB09f9fE4f79153c036044f";

    const STAKED_EVENT_SIG: &'static str =
        "0x23581b9afdc2170a53868d0b64508f096844aa55c3ad98caf14032a91c41cc52";

    const CLAIM_REGISTERED_EVENT_SIG: &'static str =
        "0x2f73775f2573d45f5b0ed0064eb65f631ac9e568a52807221c44ca9d358a9cee";

    const CLAIM_EXECUTED_EVENT_SIG: &'static str =
        "0xac96f597a44ad425c6eedf6e4c8327fd959c9d912fa8d027fb54313e59f247c8";

    const EMISSION_CHANGED_EVENT_SIG: &'static str =
        "0x0b0b5ed18390ab49777844d5fcafb9865c74095ceb3e73cc57d1fbcc926103b5";

    const MIN_STAKE_CHANGED_EVENT_SIG: &'static str =
        "0xca11c8a4c461b60c9f485404c272650c2aaae260b2067d72e9924abb68556593";

    const STAKED_LOG: &'static str = r#"{
        "address": "0x85c0d660ea89da58c05996eb8fb7a444b3543f11",
        "blockHash": "0x90c9130d55361350e0cb72fe436987fedd22111e9e554259124526ca60ddebd5",
        "blockNumber": "0x8669f5",
        "data": "0x000000000000000000000000000000000000000000000878678326eac90000000000000000000000000000000000000000000000000000000000000000000001",
        "logIndex": "0x8",
        "removed": false,
        "topics": [
            "0x23581b9afdc2170a53868d0b64508f096844aa55c3ad98caf14032a91c41cc52",
            "0x0000000000000000000000000000000000000000000000000000000000003039"
        ],
        "transactionHash": "0x3a4b2643b00b579c493f9ed171bebbac1173dd195fde1a2c4ef8f69b55a7da43",
        "transactionIndex": "0x12"
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
        "logIndex": "0x2",
        "transactionIndex": "0x0",
        "transactionHash": "0x9be0b3ab66177a80eb856772f3dff82f0d4e63c912d1f53f9ae032e68b177079",
        "blockHash": "0xc15512efc63fa6926658ba2a37b8b0930fbfb663fa7fe725b1e7f1dfaf17df54",
        "blockNumber": "0xa",
        "address": "0xead5de9c41543e4babb09f9fe4f79153c036044f",
        "data": "0x0000000000000000000000000000000000000000000000000000000000000049",
        "topics": [
            "0x749a1f8d41c63e7123adac0637a8c06d2e0d0412d454a0edf7708ba27e86c697",
            "0x000000000000000000000000000000000000000000000000000000000000e8b0"
        ],
        "type": "mined",
        "removed": false
    }"#;

    const EMISSION_CHANGED_LOG: &'static str = r#"{
        "logIndex": "0x1",
        "transactionIndex": "0x0",
        "transactionHash": "0x7af92dc418df27bc847d356e661cdbca8b3151c3a955285772a636e463c1fcc6",
        "blockHash": "0x66fc9a99f990797191c355827c0c9a8072c4cccd73efd955058cc937960158b3",
        "blockNumber": "0x8",
        "address": "0xead5de9c41543e4babb09f9fe4f79153c036044f",
        "data": "0x0000000000000000000000000000000000000000000000004dd32eacf3e5865b0e8bd531546b78a905c50cef76254047d5dcba9fa11f3f317451c3e8652e5aef",
        "topics": [
            "0x0b0b5ed18390ab49777844d5fcafb9865c74095ceb3e73cc57d1fbcc926103b5"
        ],
        "type": "mined",
        "removed": false
    }"#;

    const MIN_STAKE_CHANGED_LOG: &'static str = r#"{
        "logIndex": "0x0",
        "transactionIndex": "0x0",
        "transactionHash": "0x72309e2654dc768118b5ebfe81892a4e3429896be20c1860aa8fba43eb96ffc4",
        "blockHash": "0x9ee882cc67521ed1ad8d2ef0c7a337353a27742c365fc0865b5874b0b2bb57d8",
        "blockNumber": "0x8",
        "address": "0xead5de9c41543e4babb09f9fe4f79153c036044f",
        "data": "0x000000000000000000000000000000000000000000000878678326eac9000000000000000000000000000000000000000000000000000000000000000000c698",
        "topics": [
            "0xca11c8a4c461b60c9f485404c272650c2aaae260b2067d72e9924abb68556593"
        ],
        "type": "mined",
        "removed": false
    }"#;

    #[test]
    fn test_load_contract() {
        assert!(StakeManager::load(CONTRACT_ADDRESS).is_ok());
        assert!(StakeManager::load("not_an_address").is_err());
    }

    #[test]
    fn test_staked_log_parsing() -> anyhow::Result<()> {
        let log: web3::types::Log = serde_json::from_str(STAKED_LOG)?;

        let sm = StakeManager::load(CONTRACT_ADDRESS)?;

        match sm.parse_event(log)? {
            StakeManagerEvent::Staked(node_id, amount, tx_hash) => {
                assert_eq!(node_id, 12321);
                let base: u128 = 10;
                assert_eq!(amount, base.pow(23) as u128);
                let expected_hash = H256::from_str(
                    "0x75349046f12736cf7887f07d6e0b9b0d77334aa63b1d4f024349c72c73f9592e",
                )
                .unwrap()
                .to_fixed_bytes();
                assert_eq!(tx_hash, expected_hash);
            }
            _ => panic!("Expected StakeManagerEvent::Staked, got a different variant"),
        }

        Ok(())
    }

    #[test]
    fn test_claim_registered_log_parsing() -> anyhow::Result<()> {
        let log: web3::types::Log = serde_json::from_str(CLAIM_REGISTERED_LOG)?;

        let sm = StakeManager::load(CONTRACT_ADDRESS)?;

        match sm.parse_event(log)? {
            StakeManagerEvent::ClaimRegistered(
                node_id,
                amount,
                staker,
                start_time,
                expiry_time,
                tx_hash,
            ) => {
                assert_eq!(node_id, web3::types::U256::from_dec_str("12345").unwrap());
                assert_eq!(amount, web3::types::U256::from_dec_str("1").unwrap());
                assert_eq!(
                    staker,
                    web3::types::H160::from_str("0x9dbe382b57bcdc2aabc874130e120a3e7de09bda")
                        .unwrap()
                );
                assert_eq!(
                    start_time,
                    web3::types::U256::from_dec_str("1621387004").unwrap()
                );
                assert_eq!(
                    expiry_time,
                    web3::types::U256::from_dec_str("1621559804").unwrap()
                );
                let expected_hash = H256::from_str(
                    "0x372ce28df138b10b90dfd3defe0eb0720f033a215ef6fd3361565dba0c204aeb",
                )
                .unwrap()
                .to_fixed_bytes();
                assert_eq!(tx_hash, expected_hash)
            }
            _ => panic!("Expected Staking::ClaimRegistered, got a different variant"),
        }

        Ok(())
    }

    #[test]
    fn test_claim_executed_log_parsing() -> anyhow::Result<()> {
        let log: web3::types::Log = serde_json::from_str(CLAIM_EXECUTED_LOG)?;

        let sm = StakeManager::load(CONTRACT_ADDRESS)?;
        println!("Stake manager loaded in");

        let claim_exec_sig = sm.claim_executed_event_definition().signature();
        println!("here's the claim exec sig: {:#?}", claim_exec_sig);

        match sm.parse_event(log)? {
            StakeManagerEvent::ClaimExecuted(node_id, amount, tx_hash) => {
                assert_eq!(node_id, 59568);
                assert_eq!(amount, 73);
                let expected_hash = H256::from_str(
                    "0x9be0b3ab66177a80eb856772f3dff82f0d4e63c912d1f53f9ae032e68b177079",
                )
                .unwrap()
                .to_fixed_bytes();
                assert_eq!(tx_hash, expected_hash);
            }
            _ => panic!("Expected Staking::ClaimExecuted, got a different variant"),
        }

        Ok(())
    }

    #[test]
    fn emission_changed_log_parsing() -> anyhow::Result<()> {
        let log: web3::types::Log = serde_json::from_str(EMISSION_CHANGED_LOG)?;

        let sm = StakeManager::load(CONTRACT_ADDRESS)?;

        match sm.parse_event(log)? {
            StakeManagerEvent::EmissionChanged(
                old_emission_per_block,
                new_emission_per_block,
                tx_hash,
            ) => {
                assert_eq!(
                    old_emission_per_block,
                    U256::from_dec_str("5607877281367557723").unwrap()
                );
                assert_eq!(new_emission_per_block, U256::from_dec_str("6579443024069621580110813774705758985587161661791333414420007985268583717615").unwrap());
                let expected_hash = H256::from_str(
                    "0x7af92dc418df27bc847d356e661cdbca8b3151c3a955285772a636e463c1fcc6",
                )
                .unwrap()
                .to_fixed_bytes();
                assert_eq!(tx_hash, expected_hash);
            }
            _ => panic!("Expected Staking::EmissionChanged, got a different variant"),
        }

        Ok(())
    }

    #[test]
    fn min_stake_changed_log_parsing() -> anyhow::Result<()> {
        let log: web3::types::Log = serde_json::from_str(MIN_STAKE_CHANGED_LOG)?;

        let sm = StakeManager::load(CONTRACT_ADDRESS)?;

        match sm.parse_event(log)? {
            StakeManagerEvent::MinStakeChanged(old_min_stake, new_min_stake, tx_hash) => {
                assert_eq!(
                    old_min_stake,
                    U256::from_dec_str("40000000000000000000000").unwrap()
                );
                assert_eq!(new_min_stake, U256::from_dec_str("50840").unwrap());
                let expected_hash = H256::from_str(
                    "0x72309e2654dc768118b5ebfe81892a4e3429896be20c1860aa8fba43eb96ffc4",
                )
                .unwrap()
                .to_fixed_bytes();
                assert_eq!(tx_hash, expected_hash);
            }
            _ => panic!("Expected Staking::MinStakeChanged, got a different variant"),
        }

        Ok(())
    }

    #[test]
    fn abi_topic_sigs() -> anyhow::Result<()> {
        let sm = StakeManager::load(CONTRACT_ADDRESS)?;

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
        let expected = H256::from_str(EMISSION_CHANGED_EVENT_SIG)
            .expect("Couldn't cast emission changed event sig to H256");
        assert_eq!(emission_changed_sig, expected);

        // Min stake changed
        let min_stake_changed_sig = sm.min_stake_changed_event_definition().signature();
        let expected = H256::from_str(MIN_STAKE_CHANGED_EVENT_SIG)
            .expect("Couldn't case min stake changed event sig to H256");
        assert_eq!(min_stake_changed_sig, expected);

        Ok(())
    }
}
