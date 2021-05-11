/// Contains the information required to use the StakeManger contract as a source for
/// the EthEventStreamer
use core::str::FromStr;

use crate::eth::{decode_log_param, EventProducerError, EventSource};

use super::*;

use serde::{Deserialize, Serialize};
use web3::{
    ethabi,
    types::{BlockNumber, FilterBuilder, H160, U256},
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
    /// Loads the contract abi to get event definitions.
    pub fn load(deployed_address: &str) -> Result<Self> {
        let abi_bytes = std::include_bytes!("../abis/StakeManager.json");
        let contract = ethabi::Contract::load(abi_bytes.as_ref())?;

        Ok(Self {
            deployed_address: H160::from_str(deployed_address)?,
            contract,
        })
    }

    /// Event definition for the 'Staked' event.
    pub fn staked_event_definition(&self) -> &ethabi::Event {
        self.get_event("Staked")
            .expect("StakeManager contract should provide 'Staked' event.")
    }

    /// Event definition for the 'Staked' event.
    pub fn claimed_event_definition(&self) -> &ethabi::Event {
        self.get_event("Claimed")
            .expect("StakeManager contract should provide 'Claimed' event. ")
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
                println!("Hello, this is a staked event");
                let log = self.staked_event_definition().parse_log(raw_log)?;

                let event = StakingEvent::Staked(
                    decode_log_param(&log, "nodeID")?,
                    decode_log_param(&log, "amount")?,
                );

                Ok(event)
            }
            // TODO: Finish this
            _ if sig == self.claimed_event_definition().signature() => {
                println!("Hello, this is a claimed event");
                let event = StakingEvent::Claimed(U256([0, 1, 3, 5]), U256([123, 0, 0, 0]));
                Ok(event)
            }
            s => Err(EventProducerError::UnexpectedEvent(s))?,
        }
    }
}

#[cfg(test)]
mod tests {

    use crate::eth::EthEventStreamBuilder;

    use super::*;

    const CONTRACT_ADDRESS: &'static str = "0xEAd5De9C41543E4bAbB09f9fE4f79153c036044f";

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

    // #[test]
    // fn test_claimed_log_parsing() -> anyhow::Result<()> {
    //     let log: web3::types::Log = serde_json::from_str(CLAIMED_LOG)?;

    //     Ok(())
    // }
}
