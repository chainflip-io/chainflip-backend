use anyhow::{anyhow, Result};
use ethers::abi::RawLog;

use std::fmt::Debug;
use web3::types::{Log, H256, U256};

use super::core_h256;

/// Type for storing common (i.e. tx_hash) and specific event information
#[derive(Debug, PartialEq, Eq)]
pub struct Event<EventParameters: Debug> {
	/// The transaction hash of the transaction that emitted this event
	pub tx_hash: H256,
	/// The index number of this particular log, in the list of logs emitted by the tx_hash
	pub log_index: U256,
	/// The event specific parameters
	pub event_parameters: EventParameters,
}

impl<EventParameters: Debug> std::fmt::Display for Event<EventParameters> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "EventParameters: {:?}; tx_hash: {:#x}", self.event_parameters, self.tx_hash)
	}
}

impl<EventParameters: Debug> Event<EventParameters> {
	pub fn new_from_unparsed_logs<LogDecoder>(decode_log: &LogDecoder, log: Log) -> Result<Self>
	where
		LogDecoder: Fn(RawLog) -> Result<EventParameters>,
	{
		Ok(Self {
			tx_hash: log
				.transaction_hash
				.ok_or_else(|| anyhow!("Could not get transaction hash from ETH log"))?,
			log_index: log
				.log_index
				.ok_or_else(|| anyhow!("Could not get log index from ETH log"))?,
			event_parameters: decode_log(RawLog {
				topics: log.topics.into_iter().map(core_h256).collect(),
				data: log.data.0,
			})?,
		})
	}
}
