/// Contains the information required to use the StakeManger contract as a source for
/// the EthEventStreamer
use core::str::FromStr;

use crate::eth::{decode_log_param, EventProducerError, EventSource};

use super::*;

use serde::{Deserialize, Serialize};
use web3::{
    ethabi,
    types::{BlockNumber, FilterBuilder, H160},
};

use anyhow::Result;

/// A wrapper for the StakeManager Ethereum contract.
pub struct StakeManager {
    deployed_address: H160,
    contract: ethabi::Contract,
}

/// Represents the events that are expected from the StakeManager contract.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StakingEvent {
    /// The `Staked(nodeId, amount)` event.
    Staked(
        /// The node id of the validator that submitted the stake.
        ethabi::Uint,
        /// The amount of FLIP that was staked.
        ethabi::Uint,
    ),

    /// `Claimed(nodeId, amount)` event
    Claimed(
        /// The node id of the validator that claimed their FLIP
        ethabi::Uint,
        /// The amount of FLIP that was claimed
        ethabi::Uint,
    ),

    /// `EmissionChanged(oldEmissionPerBlock, newEmissionPerBlock)`
    EmissionChanged(
        /// Old emission per block
        ethabi::Uint,
        /// New emission per block
        ethabi::Uint,
    ),

    /// `MinStakeChanged(oldMinStake, newMinStake)`
    MinStakeChanged(
        /// Old minimum stake
        ethabi::Uint,
        /// New minimum stake
        ethabi::Uint,
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

    /// Event definition for the 'Staked' event
    pub fn claimed_event_definition(&self) -> &ethabi::Event {
        self.get_event("Claimed")
            .expect("StakeManager contract should provide 'Claimed' event.")
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
    type Event = StakingEvent;

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

                let event = StakingEvent::Staked(
                    decode_log_param(&log, "nodeID")?,
                    decode_log_param(&log, "amount")?,
                );
                Ok(event)
            }
            _ if sig == self.claimed_event_definition().signature() => {
                let log = self.claimed_event_definition().parse_log(raw_log)?;
                let event = StakingEvent::Claimed(
                    decode_log_param(&log, "nodeID")?,
                    decode_log_param(&log, "amount")?,
                );
                Ok(event)
            }
            _ if sig == self.emission_changed_event_definition().signature() => {
                let log = self
                    .emission_changed_event_definition()
                    .parse_log(raw_log)?;
                let event = StakingEvent::EmissionChanged(
                    decode_log_param(&log, "oldEmissionPerBlock")?,
                    decode_log_param(&log, "newEmissionPerBlock")?,
                );
                Ok(event)
            }
            _ if sig == self.min_stake_changed_event_definition().signature() => {
                let log = self
                    .min_stake_changed_event_definition()
                    .parse_log(raw_log)?;
                let event = StakingEvent::MinStakeChanged(
                    decode_log_param(&log, "oldMinStake")?,
                    decode_log_param(&log, "newMinStake")?,
                );
                Ok(event)
            }
            s => Err(EventProducerError::UnexpectedEvent(s))?,
        }
    }
}

#[cfg(test)]
mod tests {

    use web3::types::{H256, U256};

    use super::*;

    const CONTRACT_ADDRESS: &'static str = "0xEAd5De9C41543E4bAbB09f9fE4f79153c036044f";

    const STAKED_EVENT_SIG: &'static str =
        "0x925435fa7e37e5d9555bb18ce0d62bb9627d0846942e58e5291e9a2dded462ed";

    const CLAIMED_EVENT_SIG: &'static str =
        "0xc83b5086ce94ec8d5a88a9f5fea4b18a522bb238ed0d2d8abd959549a80c16b8";

    const EMISSION_CHANGED_EVENT_SIG: &'static str =
        "0x0b0b5ed18390ab49777844d5fcafb9865c74095ceb3e73cc57d1fbcc926103b5";

    const MIN_STAKE_CHANGED_EVENT_SIG: &'static str =
        "0xca11c8a4c461b60c9f485404c272650c2aaae260b2067d72e9924abb68556593";

    const STAKED_LOG: &'static str = r#"{
        "logIndex": "0x2",
        "transactionIndex": "0x0",
        "transactionHash": "0x75349046f12736cf7887f07d6e0b9b0d77334aa63b1d4f024349c72c73f9592e",
        "blockHash": "0x76cc3567874b42ed341a06b157beb9f98e3afc000c7dd29438c8f5be36080bf2",
        "blockNumber": "0x8",
        "address": "0xead5de9c41543e4babb09f9fe4f79153c036044f",
        "data": "0x00000000000000000000000000000000000000000000152d02c7e14af6800000",
        "topics": [
            "0x925435fa7e37e5d9555bb18ce0d62bb9627d0846942e58e5291e9a2dded462ed",
            "0x0000000000000000000000000000000000000000000000000000000000003021"
        ],
        "type": "mined",
        "removed": false
    }"#;

    const CLAIMED_LOG: &'static str = r#"{
        "logIndex": "0x2",
        "transactionIndex": "0x0",
        "transactionHash": "0x75349046f12736cf7887f07d6e0b9b0d77334aa63b1d4f024349c72c73f9592e",
        "blockHash": "0x76cc3567874b42ed341a06b157beb9f98e3afc000c7dd29438c8f5be36080bf2",
        "blockNumber": "0x8",
        "address": "0xead5de9c41543e4babb09f9fe4f79153c036044f",
        "data": "0x000000000000000000000000000000000000000000000817090f1518090e0303",
        "topics": [
            "0xc83b5086ce94ec8d5a88a9f5fea4b18a522bb238ed0d2d8abd959549a80c16b8",
            "0x0000000000000000000000000000000000000000000000000000000000003039"
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
            StakingEvent::Staked(node_id, amount) => {
                assert_eq!(node_id, web3::types::U256::from(12321));
                assert_eq!(amount, web3::types::U256::exp10(23));
            }
            _ => panic!("Expected StakingEvent::Staked, got a different variant"),
        }

        Ok(())
    }

    #[test]
    fn test_claimed_log_parsing() -> anyhow::Result<()> {
        let log: web3::types::Log = serde_json::from_str(CLAIMED_LOG)?;

        let sm = StakeManager::load(CONTRACT_ADDRESS)?;

        match sm.parse_event(log)? {
            StakingEvent::Claimed(node_id, amount) => {
                assert_eq!(node_id, web3::types::U256::from_dec_str("12345").unwrap());
                assert_eq!(
                    amount,
                    web3::types::U256::from_dec_str("38203859740316448719619").unwrap()
                );
            }
            _ => panic!("Expected Staking::Claimed, got a different variant"),
        }

        Ok(())
    }

    #[test]
    fn emission_changed_log_parsing() -> anyhow::Result<()> {
        let log: web3::types::Log = serde_json::from_str(EMISSION_CHANGED_LOG)?;

        let sm = StakeManager::load(CONTRACT_ADDRESS)?;

        match sm.parse_event(log)? {
            StakingEvent::EmissionChanged(old_emission_per_block, new_emission_per_block) => {
                assert_eq!(
                    old_emission_per_block,
                    U256::from_dec_str("5607877281367557723").unwrap()
                );
                assert_eq!(new_emission_per_block, U256::from_dec_str("6579443024069621580110813774705758985587161661791333414420007985268583717615").unwrap());
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
            StakingEvent::MinStakeChanged(old_min_stake, new_min_stake) => {
                assert_eq!(
                    old_min_stake,
                    U256::from_dec_str("40000000000000000000000").unwrap()
                );
                assert_eq!(new_min_stake, U256::from_dec_str("50840").unwrap());
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

        // Claimed event
        let claimed_sig = sm.claimed_event_definition().signature();
        let expected =
            H256::from_str(CLAIMED_EVENT_SIG).expect("Couldn't cast claimed event sig to H256");
        assert_eq!(claimed_sig, expected);

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
