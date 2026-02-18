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

use crate::{
	evm::{
		cached_rpc::AddressCheckerRetryRpcApiWithResult, event::Event, retry_rpc::EvmRetryRpcApi,
		rpc::address_checker::AddressState,
	},
	witness::evm::{
		EvmAddressStateClient, EvmBlockQuery, EvmDepositChannelWitnessingConfig, EvmEventClient,
	},
};
use cf_chains::{evm::EvmChain, DepositChannel};
use ethers::{abi::ethereum_types::BloomInput, types::Bloom};
use futures::try_join;
use pallet_cf_elections::electoral_systems::block_height_witnesser::ChainTypes;
use std::collections::{HashMap, HashSet};

use super::{super::common::chain_source::Header, vault::VaultEvents};
use anyhow::{ensure, Result};
use sp_core::{H160, H256};

// ----- implementation ------

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

pub async fn address_states<EvmCachingClient>(
	eth_rpc: &EvmCachingClient,
	address_checker_address: H160,
	parent_hash: H256,
	hash: H256,
	addresses: Vec<H160>,
) -> Result<HashMap<H160, (AddressState, AddressState)>, anyhow::Error>
where
	EvmCachingClient: AddressCheckerRetryRpcApiWithResult + Send + Sync + Clone,
{
	if addresses.is_empty() {
		return Ok(Default::default());
	}
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
		.zip(previous_address_states.into_iter().zip(address_states))
		.collect::<HashMap<H160, _>>())
}

/// Generic helper function for deposit channel witnessing
///
/// This function handles the common logic for witnessing deposit channels on EVM chains.
/// The chain-specific parts (native asset, ERC20 asset handling) are provided via parameters.
pub async fn witness_deposit_channels_generic<
	Chain: EvmChain<ChainAsset: std::hash::Hash>,
	CT: ChainTypes<ChainBlockHash = H256>,
	Client: EvmEventClient<CT> + EvmAddressStateClient<CT>,
>(
	client: &Client,
	config: &EvmDepositChannelWitnessingConfig<Chain>,
	query: &Client::BlockQuery,
	deposit_addresses: Vec<DepositChannel<Chain>>,
) -> Result<Vec<pallet_cf_ingress_egress::DepositWitness<Chain>>>
where
	Chain::ChainAmount: TryFrom<sp_core::U256>,
	<Chain::ChainAmount as TryFrom<sp_core::U256>>::Error: std::fmt::Debug,
{
	use super::evm_deposits::eth_ingresses_at_block;
	use itertools::Itertools;
	use pallet_cf_ingress_egress::DepositWitness;

	let address_checker_address = config.address_checker_address;

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

	// let block_start = *block_height.into_range_inclusive().start();
	let block_start = query.get_lowest_block_height_of_query();
	let undeployed_addresses: Vec<H160> = eth_deposit_channels
		.into_iter()
		.filter_map(|(address, deployment_status)| {
			if deployment_status.is_deployed_before(&block_start) {
				None
			} else {
				Some(address)
			}
		})
		.collect();

	let (undeployed_addr_states, events) = futures::try_join!(
		client.address_states(address_checker_address, query.clone(), undeployed_addresses,),
		async {
			let events =
				client.events_from_block_query(&config.vault_contract, query.clone()).await?;
			Ok::<_, anyhow::Error>(
				events
					.into_iter()
					.filter_map(|event| match event.event_parameters {
						VaultEvents::FetchedNativeFilter(inner_event)
							if eth_addresses.contains(&inner_event.sender) =>
							Some((inner_event, event.tx_hash)),
						_ => None,
					})
					.collect::<Vec<_>>(),
			)
		},
	)?;

	let eth_ingresses = eth_ingresses_at_block(undeployed_addr_states, events)?;

	let mut erc20_ingresses: Vec<DepositWitness<Chain>> = Vec::new();

	// Handle each asset type separately with its specific event type
	for (asset, deposit_channels) in erc20_deposit_channels {
		let event_source = config.supported_assets.get(&asset).ok_or_else(|| {
			anyhow::anyhow!("Tried to get erc20 events for unsupported asset: {asset:?}")
		})?;

		let events = client.events_from_block_query(event_source, query.clone()).await?;

		let asset_ingresses = events
			.into_iter()
			.filter_map(|event| match event.event_parameters {
				super::erc20_deposits::Erc20Events::TransferFilter { to, value, from: _ }
					if deposit_channels.contains(&to) =>
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
			})
			.collect::<Vec<_>>();

		erc20_ingresses.extend(asset_ingresses);
	}

	Ok(eth_ingresses
		.into_iter()
		.map(|(to_addr, value, tx_hashes)| DepositWitness {
			deposit_address: to_addr,
			asset: Chain::GAS_ASSET,
			amount: value.try_into().expect("Ingress witness transfer value should fit u128"),
			deposit_details: Chain::DepositDetails { tx_hashes },
		})
		.chain(erc20_ingresses)
		.sorted_by_key(|deposit_witness| deposit_witness.deposit_address)
		.collect())
}
