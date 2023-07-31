use ethers::abi::RawLog;

use std::fmt::Debug;

use crate::eth::retry_rpc::EthersRetryRpcApi;

use super::chain_source::Header;
use anyhow::{anyhow, Result};
use sp_core::{H160, H256, U256};

use ethers::{
	abi::ethereum_types::BloomInput,
	types::{Bloom, Log},
};

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

impl<EventParameters: Debug + ethers::contract::EthLogDecode> Event<EventParameters> {
	pub fn new_from_unparsed_logs(log: Log) -> Result<Self> {
		Ok(Self {
			tx_hash: log
				.transaction_hash
				.ok_or_else(|| anyhow!("Could not get transaction hash from ETH log"))?,
			log_index: log
				.log_index
				.ok_or_else(|| anyhow!("Could not get log index from ETH log"))?,
			event_parameters: EventParameters::decode_log(&RawLog {
				topics: log.topics.into_iter().collect(),
				data: log.data.to_vec(),
			})?,
		})
	}
}

pub async fn events_at_block<EventParameters, EthRpcClient>(
	header: Header<u64, H256, Bloom>,
	contract_address: H160,
	eth_rpc: &EthRpcClient,
) -> Result<Vec<Event<EventParameters>>>
where
	EventParameters: std::fmt::Debug + ethers::contract::EthLogDecode + Send + Sync + 'static,
	EthRpcClient: EthersRetryRpcApi,
{
	let mut contract_bloom = Bloom::default();
	contract_bloom.accrue(BloomInput::Raw(&contract_address.0));

	// if we have logs for this block, fetch them.
	if header.data.contains_bloom(&contract_bloom) {
		eth_rpc
			.get_logs(header.hash, contract_address)
			.await
			.into_iter()
			.map(|unparsed_log| -> anyhow::Result<Event<EventParameters>> {
				Event::<EventParameters>::new_from_unparsed_logs(unparsed_log)
			})
			.collect::<anyhow::Result<Vec<_>>>()
	} else {
		// we know there won't be interesting logs, so don't fetch for events
		anyhow::Result::Ok(vec![])
	}
}
