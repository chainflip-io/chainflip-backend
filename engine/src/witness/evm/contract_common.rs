// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use crate::evm::{
	cached_rpc::{AddressCheckerRetryRpcApiWithResult, EvmRetryRpcApiWithResult},
	rpc::address_checker::AddressState,
};
use cf_chains::witness_period::SaturatingStep;
use ethers::abi::RawLog;
use futures::try_join;
use pallet_cf_elections::electoral_systems::{
	block_height_witnesser::ChainTypes, block_witnesser::state_machine::EngineElectionType,
};
use std::fmt::Debug;

use crate::evm::{
	cached_rpc::EvmCachingClient, retry_rpc::EvmRetryRpcApi, rpc::EvmRpcSigningClient,
};

use super::super::common::chain_source::Header;
use anyhow::{anyhow, ensure, Result};
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

pub async fn events_at_block2<Chain, EventParameters, EvmRpcClient>(
	header: Header<u64, H256, Bloom>,
	contract_address: H160,
	eth_rpc: &EvmRpcClient,
) -> Result<Vec<Event<EventParameters>>>
where
	Chain: cf_chains::Chain<ChainBlockNumber = u64>,
	EventParameters: std::fmt::Debug + ethers::contract::EthLogDecode + Send + Sync + 'static,
	EvmRpcClient: EvmRetryRpcApi,
{
	assert!(Chain::is_block_witness_root(header.index));
	if Chain::WITNESS_PERIOD == 1 {
		let mut contract_bloom = Bloom::default();
		contract_bloom.accrue(BloomInput::Raw(&contract_address.0));

		// if we have logs for this block, fetch them.
		if header.data.contains_bloom(&contract_bloom) {
			eth_rpc.get_logs(header.hash, contract_address).await
		} else {
			// we know there won't be interesting logs, so don't fetch for events
			vec![]
		}
	} else {
		eth_rpc
			.get_logs_range(Chain::block_witness_range(header.index), contract_address)
			.await
	}
	.into_iter()
	.map(|unparsed_log| -> anyhow::Result<Event<EventParameters>> {
		Event::<EventParameters>::new_from_unparsed_logs(unparsed_log)
	})
	.collect::<anyhow::Result<Vec<_>>>()
}

pub struct Block {
	pub hash: H256,
	pub parent_hash: H256,
	pub bloom: Option<Bloom>,
}

pub async fn query_election_block<
	CT: ChainTypes<ChainBlockHash = H256>,
	C: cf_chains::Chain<ChainBlockNumber = u64>,
>(
	client: &EvmCachingClient<EvmRpcSigningClient>,
	block_height: CT::ChainBlockNumber,
	election_type: EngineElectionType<CT>,
) -> Result<(Block, Option<CT::ChainBlockHash>)> {
	match election_type {
		EngineElectionType::ByHash(hash) => {
			let block = client.block_by_hash(hash).await?;
			if let Some(block_hash) = block.hash {
				if block_hash != hash {
					return Err(anyhow::anyhow!(
						"Block hash from RPC ({}) doesn't match election block hash: {}",
						block_hash,
						hash
					));
				}
				Ok((
					Block {
						hash: block_hash,
						parent_hash: if C::WITNESS_PERIOD == 1 {
							block.parent_hash
						} else {
							let block_number_range = block
								.number
								.ok_or(anyhow::anyhow!("No block number"))?
								.low_u64()
								.into_range_inclusive();
							client.block((*block_number_range.start()).into()).await?.parent_hash
						},
						bloom: if C::WITNESS_PERIOD == 1 {
							Some(block.logs_bloom.unwrap_or(Bloom::repeat_byte(0xFFu8)))
						} else {
							None
						},
					},
					None,
				))
			} else {
				Err(anyhow::anyhow!(
					"Block number or hash is none for block number: {:?}",
					block_height
				))
			}
		},
		EngineElectionType::BlockHeight { submit_hash } => {
			let block_number_range = block_height.into_range_inclusive();
			let block = client.block((*block_number_range.end()).into()).await?;
			if let (Some(block_number), Some(block_hash)) = (block.number, block.hash) {
				if block_number.as_u64() != *block_number_range.end() {
					return Err(anyhow::anyhow!(
						"Block number from RPC ({}) doesn't match election block height: {:?}",
						block_number,
						block_height
					));
				}
				Ok((
					Block {
						hash: block_hash,
						parent_hash: if C::WITNESS_PERIOD == 1 {
							block.parent_hash
						} else {
							client.block((*block_number_range.start()).into()).await?.parent_hash
						},
						bloom: if C::WITNESS_PERIOD == 1 {
							Some(block.logs_bloom.unwrap_or(Bloom::repeat_byte(0xFFu8)))
						} else {
							None
						},
					},
					if submit_hash { Some(block_hash) } else { None },
				))
			} else {
				Err(anyhow::anyhow!(
					"Block number or hash is none for block number: {:?}",
					block_height
				))
			}
		},
	}
}

pub async fn address_states<EvmCachingClient>(
	eth_rpc: &EvmCachingClient,
	address_checker_address: H160,
	parent_hash: H256,
	hash: H256,
	addresses: Vec<H160>,
) -> Result<impl Iterator<Item = (H160, (AddressState, AddressState))>, anyhow::Error>
where
	EvmCachingClient: AddressCheckerRetryRpcApiWithResult + Send + Sync + Clone,
{
	let (previous_address_states, address_states) = try_join!(
		eth_rpc.address_states(parent_hash, address_checker_address, addresses.clone()),
		eth_rpc.address_states(hash, address_checker_address, addresses.clone())
	)?;

	ensure!(
		addresses.len() == previous_address_states.len() &&
			previous_address_states.len() == address_states.len()
	);

	Ok(addresses
		.into_iter()
		.zip(previous_address_states.into_iter().zip(address_states)))
}

pub async fn events_at_block<Chain, EventParameters, EvmCachingClient>(
	data: Option<Bloom>,
	block_number: Chain::ChainBlockNumber,
	block_hash: H256,
	contract_address: H160,
	eth_rpc: &EvmCachingClient,
) -> Result<Vec<Event<EventParameters>>>
where
	Chain: cf_chains::Chain<ChainBlockNumber = u64>,
	EventParameters: std::fmt::Debug + ethers::contract::EthLogDecode + Send + Sync + 'static,
	EvmCachingClient: EvmRetryRpcApiWithResult,
{
	assert!(Chain::is_block_witness_root(block_number));
	if Chain::WITNESS_PERIOD == 1 {
		let mut contract_bloom = Bloom::default();
		contract_bloom.accrue(BloomInput::Raw(&contract_address.0));
		let data = data.ok_or(anyhow::anyhow!(
			"We should always have a bloom for chains with WITNESS_PERIOD==1"
		))?;
		// if we have logs for this block, fetch them.
		if data.contains_bloom(&contract_bloom) {
			eth_rpc.get_logs(block_hash, contract_address).await?
		} else {
			// we know there won't be interesting logs, so don't fetch for events
			vec![]
		}
	} else {
		eth_rpc
			.get_logs_range(Chain::block_witness_range(block_number), contract_address)
			.await?
	}
	.into_iter()
	.map(|unparsed_log| -> anyhow::Result<Event<EventParameters>> {
		Event::<EventParameters>::new_from_unparsed_logs(unparsed_log)
	})
	.collect::<anyhow::Result<Vec<_>>>()
}
