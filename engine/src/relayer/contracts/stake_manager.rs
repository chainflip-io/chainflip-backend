use crate::relayer::EventSource;

use super::*;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use web3::{
    api::Eth,
    ethabi::{self, Event, RawTopicFilter, TopicFilter},
    types::{FilterBuilder, H160},
};

lazy_static! {
    static ref CONTRACT_ADDRESS: H160 =
        H160::from_str("0xd59D75482465E7442e59f73320152dab5ac458d7").unwrap();
}

/// A wrapper for the StakeManager Ethereum contract.
pub struct StakeManager {
    contract: ethabi::Contract,
}

/// Represents the events that are expected from the StakeManager contract.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StakingEvent {
    Staked {
        node_id: ethabi::Uint,
        amount: ethabi::Uint,
    },
    Unknown,
}

impl StakeManager {
    /// Loads the contract abi to get event definitions.
    pub fn load() -> Result<Self> {
        let abi_bytes = std::include_bytes!("../../../abis/StakeManager.json");
        let contract = ethabi::Contract::load(abi_bytes.as_ref())?;

        Ok(Self { contract })
    }

    /// Event definition for the 'Staked' event.
    pub fn staked_event(&self) -> &Event {
        self.get_event("Staked")
            .expect("StakeManager contract should provide 'Staked' event.")
    }

    fn get_event(&self, name: &str) -> Result<&Event> {
        Ok(self.contract.event(name)?)
    }

    /// Convert to a web3-style callable contract with apis for calling contract functions.
    pub fn to_callable_contract<T: web3::Transport>(
        self,
        client: Eth<T>,
    ) -> web3::contract::Contract<T> {
        web3::contract::Contract::<T>::new(client, *CONTRACT_ADDRESS, self.contract)
    }
}

impl EventSource for StakeManager {
    type Event = StakingEvent;

    fn topic_filter_for_event(&self, name: &str) -> Result<TopicFilter> {
        let f = self.get_event(name)?.filter(RawTopicFilter::default())?;
        Ok(f)
    }

    fn filter_builder(&self, block: BlockNumber) -> FilterBuilder {
        FilterBuilder::default()
            .from_block(block)
            .address(vec![*CONTRACT_ADDRESS])
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

        log::debug!("Parsing event with signature: {}", sig);

        match sig {
            _ if sig == self.staked_event().signature() => {
                let log = self.staked_event().parse_log(raw_log)?;

                let event = StakingEvent::Staked {
                    node_id: decode_log_param(&log, "nodeID")?,
                    amount: decode_log_param(&log, "amount")?,
                };

                Ok(event)
            }
            s => Err(EventProducerError::UnexpectedEvent(s))?,
        }
    }
}
