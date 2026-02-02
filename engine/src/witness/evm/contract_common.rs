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
	cached_rpc::{AddressCheckerRetryRpcApiWithResult, EvmCachingClient, EvmRetryRpcApiWithResult},
	retry_rpc::EvmRetryRpcApi,
	rpc::{address_checker::AddressState, EvmRpcSigningClient},
};
use cf_chains::{evm::DeploymentStatus, witness_period::SaturatingStep, DepositChannel};
use ethers::{
	abi::{ethereum_types::BloomInput, RawLog},
	types::{Bloom, Log},
};
use futures::try_join;
use pallet_cf_elections::electoral_systems::{
	block_height_witnesser::ChainTypes, block_witnesser::state_machine::EngineElectionType,
};
use std::{
	collections::{HashMap, HashSet},
	fmt::Debug,
};

use super::{super::common::chain_source::Header, vault::VaultEvents};
use anyhow::{anyhow, ensure, Result};
use sp_core::{H160, H256, U256};

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

pub async fn events_at_block_deprecated<Chain, EventParameters, EvmRpcClient>(
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

pub struct EvmBlockHeader {
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
) -> Result<(EvmBlockHeader, Option<CT::ChainBlockHash>)> {
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
					EvmBlockHeader {
						hash: block_hash,
						parent_hash: if C::WITNESS_PERIOD == 1 {
							block.parent_hash
						} else {
							let block_number_range = C::block_witness_range(
								block.number.ok_or(anyhow::anyhow!("No block number"))?.low_u64(),
							);
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
					EvmBlockHeader {
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

pub async fn events_at_block<Chain, EventParameters, CT: ChainTypes, EvmCachingClient>(
	data: Option<Bloom>,
	block_number: CT::ChainBlockNumber,
	block_hash: H256,
	contract_address: H160,
	eth_rpc: &EvmCachingClient,
) -> Result<Vec<Event<EventParameters>>>
where
	Chain: cf_chains::Chain<ChainBlockNumber = u64>,
	EventParameters: std::fmt::Debug + ethers::contract::EthLogDecode + Send + Sync + 'static,
	EvmCachingClient: EvmRetryRpcApiWithResult,
{
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
			.get_logs_range(block_number.into_range_inclusive(), contract_address)
			.await?
	}
	.into_iter()
	.map(|unparsed_log| -> anyhow::Result<Event<EventParameters>> {
		Event::<EventParameters>::new_from_unparsed_logs(unparsed_log)
	})
	.collect::<anyhow::Result<Vec<_>>>()
}

/// Trait for deposit channel witnesser configuration
#[async_trait::async_trait]
pub trait DepositChannelWitnesserConfig<Chain: cf_chains::Chain, CT: ChainTypes> {
	fn client(&self) -> &EvmCachingClient<EvmRpcSigningClient>;
	fn address_checker_address(&self) -> H160;
	fn vault_address(&self) -> H160;
	async fn get_events_for_erc20_asset(
		&self,
		asset: Chain::ChainAsset,
		bloom: Option<Bloom>,
		block_height: CT::ChainBlockNumber,
		block_hash: H256,
	) -> Result<Option<Vec<Event<super::erc20_deposits::Erc20Events>>>>;
}
/// Generic helper function for deposit channel witnessing
///
/// This function handles the common logic for witnessing deposit channels on EVM chains.
/// The chain-specific parts (native asset, ERC20 asset handling) are provided via parameters.
pub async fn witness_deposit_channels_generic<
	Chain: cf_chains::Chain<
		ChainBlockNumber = u64,
		ChainAccount = H160,
		ChainAsset: std::hash::Hash,
		DepositDetails = cf_chains::evm::DepositDetails,
		DepositChannelState = DeploymentStatus,
	>,
	CT: ChainTypes<ChainBlockHash = H256>,
	Config: DepositChannelWitnesserConfig<Chain, CT>,
>(
	config: &Config,
	block_height: CT::ChainBlockNumber,
	election_type: EngineElectionType<CT>,
	deposit_addresses: Vec<DepositChannel<Chain>>,
) -> Result<(Vec<pallet_cf_ingress_egress::DepositWitness<Chain>>, Option<CT::ChainBlockHash>)>
where
	Chain::ChainAmount: TryFrom<sp_core::U256>,
	<Chain::ChainAmount as TryFrom<sp_core::U256>>::Error: std::fmt::Debug,
{
	use super::evm_deposits::eth_ingresses_at_block;
	use itertools::Itertools;
	use pallet_cf_ingress_egress::DepositWitness;

	let client = config.client();
	let address_checker_address = config.address_checker_address();
	let vault_address = config.vault_address();

	let (block, return_block_hash) =
		query_election_block::<CT, Chain>(client, block_height, election_type).await?;

	let (eth_deposit_channels, erc20_deposit_channels): (Vec<_>, HashMap<_, Vec<_>>) =
		deposit_addresses.into_iter().fold(
			(Vec::new(), HashMap::new()),
			|(mut eth, mut erc20), deposit_channel| {
				let asset = deposit_channel.asset;
				let address = deposit_channel.address;
				if asset == Chain::GAS_ASSET {
					eth.push((address, deposit_channel.state));
				} else {
					erc20.entry(asset).or_insert_with(Vec::new).push(address);
				}
				(eth, erc20)
			},
		);
	let eth_addresses: HashSet<H160> =
		eth_deposit_channels.iter().map(|(address, _state)| *address).collect();

	let block_start = *block_height.into_range_inclusive().start();
	let (deployed_addresses, undeployed_addresses): (HashSet<H160>, Vec<H160>) =
		eth_deposit_channels.iter().fold(
			(HashSet::new(), Vec::new()),
			|(mut deployed, mut undeployed), (address, deployment_status)| {
				if deployment_status.deployed_before(&block_start) {
					deployed.insert(*address);
				} else {
					undeployed.push(*address);
				}
				(deployed, undeployed)
			},
		);

	let events_fut = async {
		Ok::<_, anyhow::Error>(
			events_at_block::<Chain, VaultEvents, CT, _>(
				block.bloom,
				block_height,
				block.hash,
				vault_address,
				client,
			)
			.await?
			.into_iter()
			.filter_map(|event| match event.event_parameters {
				VaultEvents::FetchedNativeFilter(inner_event)
					if eth_addresses.contains(&inner_event.sender) =>
					Some((inner_event, event.tx_hash)),
				_ => None,
			})
			.collect::<Vec<_>>(),
		)
	};

	let eth_ingresses = if undeployed_addresses.is_empty() {
		eth_ingresses_at_block(None::<std::iter::Empty<_>>, events_fut.await?)?
	} else {
		let (undeployed_addr_states, events) = futures::try_join!(
			address_states(
				client,
				address_checker_address,
				block.parent_hash,
				block.hash,
				undeployed_addresses,
			),
			events_fut,
		)?;

		let (deployed_addr_events, undeployed_addr_events): (Vec<_>, Vec<_>) = events
			.into_iter()
			.partition(|(event, _)| deployed_addresses.contains(&event.sender));

		let mut ingresses =
			eth_ingresses_at_block(Some(undeployed_addr_states), undeployed_addr_events)?;
		ingresses
			.extend(eth_ingresses_at_block(None::<std::iter::Empty<_>>, deployed_addr_events)?);
		ingresses
	};

	let mut erc20_ingresses: Vec<DepositWitness<Chain>> = Vec::new();

	// Handle each asset type separately with its specific event type
	for (asset, deposit_channels) in erc20_deposit_channels {
		if let Some(events) = config
			.get_events_for_erc20_asset(asset, block.bloom, block_height, block.hash)
			.await?
		{
			let asset_ingresses = events
			.into_iter()
			.filter_map(|event| {
				match event.event_parameters {
					super::erc20_deposits::Erc20Events::TransferFilter{to, value, from: _ } if deposit_channels.contains(&to) =>
						Some(DepositWitness {
							deposit_address: to,
							amount: value.try_into().expect(
								"Any ERC20 tokens we support should have amounts that fit into a u128",
							),
							asset,
							deposit_details: Chain::DepositDetails {
								tx_hashes: Some(vec![event.tx_hash]),
							},
						}),
					_ => None,
				}
			})
			.collect::<Vec<_>>();

			erc20_ingresses.extend(asset_ingresses);
		}
	}

	Ok((
		eth_ingresses
			.into_iter()
			.map(|(to_addr, value, tx_hashes)| DepositWitness {
				deposit_address: to_addr,
				asset: Chain::GAS_ASSET,
				amount: value.try_into().expect("Ingress witness transfer value should fit u128"),
				deposit_details: Chain::DepositDetails { tx_hashes },
			})
			.chain(erc20_ingresses)
			.sorted_by_key(|deposit_witness| deposit_witness.deposit_address)
			.collect(),
		return_block_hash,
	))
}
