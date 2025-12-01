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
	base_rpc_api::RawRpcApi, extrinsic_api::signed::SignedExtrinsicApi, BaseRpcClient, BlockInfo,
	StateChainClient,
};
use codec::{Decode, Encode};
use frame_support::instances::*;
use pallet_cf_elections::{ElectionIdentifierOf, ElectoralDataFor, VoteOf};
use state_chain_runtime::{ArbitrumInstance, BitcoinInstance, EthereumInstance, SolanaInstance};
use std::collections::{BTreeMap, BTreeSet};
use tracing::error;

pub trait ElectoralApi<Instance: 'static>
where
	state_chain_runtime::Runtime: pallet_cf_elections::Config<Instance>,
{
	/// Returns information about all the current elections from the perspective of this validator.
	fn electoral_data(
		&self,
		block: BlockInfo,
	) -> impl std::future::Future<
		Output = Option<ElectoralDataFor<state_chain_runtime::Runtime, Instance>>,
	> + Send
	       + 'static;

	/// Returns the subset of proposed_votes that need to be submitted.
	fn filter_votes(
		&self,
		proposed_votes: BTreeMap<
			ElectionIdentifierOf<<state_chain_runtime::Runtime as pallet_cf_elections::Config<Instance>>::ElectoralSystemRunner>,
			VoteOf<<state_chain_runtime::Runtime as pallet_cf_elections::Config<Instance>>::ElectoralSystemRunner>,
		>,
	) -> impl std::future::Future<Output = BTreeSet<ElectionIdentifierOf<<state_chain_runtime::Runtime as pallet_cf_elections::Config<Instance>>::ElectoralSystemRunner>>> + Send + 'static;
}

impl<
		RawRpcClient: RawRpcApi + Send + Sync + 'static,
		SignedExtrinsicClient: SignedExtrinsicApi + Send + Sync + 'static,
	> ElectoralApi<Instance5>
	for StateChainClient<SignedExtrinsicClient, BaseRpcClient<RawRpcClient>>
{
	fn electoral_data(
		&self,
		block: BlockInfo,
	) -> impl std::future::Future<
		Output = Option<ElectoralDataFor<state_chain_runtime::Runtime, SolanaInstance>>,
	> + Send
	       + 'static {
		let base_rpc_client = self.base_rpc_client.clone();
		let account_id = self.signed_extrinsic_client.account_id();
		async move {
			base_rpc_client
				.raw_rpc_client
				.cf_solana_electoral_data(account_id, Some(block.hash))
				.await
				.map_err(anyhow::Error::from)
				.and_then(|electoral_data| <Option<ElectoralDataFor<state_chain_runtime::Runtime, SolanaInstance>> as Decode>::decode(&mut &electoral_data[..]).map_err(Into::into))
				.inspect_err(|error| {
					error!("Failure in electoral_data rpc: '{}'", error);
				})
				.ok()
				.flatten()
		}
	}

	fn filter_votes(
		&self,
		proposed_votes: BTreeMap<
			ElectionIdentifierOf<<state_chain_runtime::Runtime as pallet_cf_elections::Config<SolanaInstance>>::ElectoralSystemRunner>,
			VoteOf<<state_chain_runtime::Runtime as pallet_cf_elections::Config<SolanaInstance>>::ElectoralSystemRunner>,
		>,
	) -> impl std::future::Future<Output = BTreeSet<ElectionIdentifierOf<<state_chain_runtime::Runtime as pallet_cf_elections::Config<SolanaInstance>>::ElectoralSystemRunner>>> + Send + 'static{
		let base_rpc_client = self.base_rpc_client.clone();
		let account_id = self.signed_extrinsic_client.account_id();
		async move {
			base_rpc_client
				.raw_rpc_client
				.cf_solana_filter_votes(account_id, proposed_votes.encode(), None)
				.await
				.map_err(anyhow::Error::from)
				.and_then(|electoral_data| {
					<BTreeSet<
						ElectionIdentifierOf<
							<state_chain_runtime::Runtime as pallet_cf_elections::Config<
								SolanaInstance,
							>>::ElectoralSystemRunner,
						>,
					> as Decode>::decode(&mut &electoral_data[..])
					.map_err(Into::into)
				})
				.inspect_err(|error| {
					error!("Failure in filter_votes rpc: '{}'", error);
				})
				.unwrap_or_default()
		}
	}
}

impl<
		RawRpcClient: RawRpcApi + Send + Sync + 'static,
		SignedExtrinsicClient: SignedExtrinsicApi + Send + Sync + 'static,
	> ElectoralApi<Instance3>
	for StateChainClient<SignedExtrinsicClient, BaseRpcClient<RawRpcClient>>
{
	fn electoral_data(
		&self,
		block: BlockInfo,
	) -> impl std::future::Future<
		Output = Option<ElectoralDataFor<state_chain_runtime::Runtime, BitcoinInstance>>,
	> + Send
	       + 'static {
		let base_rpc_client = self.base_rpc_client.clone();
		let account_id = self.signed_extrinsic_client.account_id();
		async move {
			base_rpc_client
				.raw_rpc_client
				.cf_bitcoin_electoral_data(account_id, Some(block.hash))
				.await
				.map_err(anyhow::Error::from)
				.and_then(|electoral_data| {
					<Option<ElectoralDataFor<state_chain_runtime::Runtime,
		BitcoinInstance>> as Decode>::decode(&mut &electoral_data[..]).map_err(Into::into)
				})
				.inspect_err(|error| {
					error!("Failure in electoral_data rpc: '{}'", error);
				})
				.ok()
				.flatten()
		}
	}

	fn filter_votes(
		&self,
		proposed_votes: BTreeMap<
			ElectionIdentifierOf<<state_chain_runtime::Runtime as pallet_cf_elections::Config<BitcoinInstance>>::ElectoralSystemRunner>,
			VoteOf<<state_chain_runtime::Runtime as pallet_cf_elections::Config<BitcoinInstance>>::ElectoralSystemRunner>,
			>
	) -> impl std::future::Future<Output =
	BTreeSet<ElectionIdentifierOf<<state_chain_runtime::Runtime as pallet_cf_elections::Config<BitcoinInstance>>::ElectoralSystemRunner>>> + Send + 'static{
		let base_rpc_client = self.base_rpc_client.clone();
		let account_id = self.signed_extrinsic_client.account_id();
		async move {
			base_rpc_client
				.raw_rpc_client
				.cf_bitcoin_filter_votes(account_id, proposed_votes.encode(), None)
				.await
				.map_err(anyhow::Error::from)
				.and_then(|electoral_data| {
					<BTreeSet<
						ElectionIdentifierOf<
							<state_chain_runtime::Runtime as pallet_cf_elections::Config<
								BitcoinInstance,
							>>::ElectoralSystemRunner,
						>,
					> as Decode>::decode(&mut &electoral_data[..])
					.map_err(Into::into)
				})
				.inspect_err(|error| {
					error!("Failure in filter_votes rpc: '{}'", error);
				})
				.unwrap_or_default()
		}
	}
}

impl<
		RawRpcClient: RawRpcApi + Send + Sync + 'static,
		SignedExtrinsicClient: SignedExtrinsicApi + Send + Sync + 'static,
	> ElectoralApi<Instance1>
	for StateChainClient<SignedExtrinsicClient, BaseRpcClient<RawRpcClient>>
{
	fn electoral_data(
		&self,
		block: BlockInfo,
	) -> impl std::future::Future<
		Output = Option<ElectoralDataFor<state_chain_runtime::Runtime, EthereumInstance>>,
	> + Send
	       + 'static {
		let base_rpc_client = self.base_rpc_client.clone();
		let account_id = self.signed_extrinsic_client.account_id();
		async move {
			base_rpc_client
				.raw_rpc_client
				.cf_ethereum_electoral_data(account_id, Some(block.hash))
				.await
				.map_err(anyhow::Error::from)
				.and_then(|electoral_data| <Option<ElectoralDataFor<state_chain_runtime::Runtime, EthereumInstance>> as Decode>::decode(&mut &electoral_data[..]).map_err(Into::into))
				.inspect_err(|error| {
					error!("Failure in electoral_data rpc: '{}'", error);
				})
				.ok()
				.flatten()
		}
	}

	fn filter_votes(
		&self,
		proposed_votes: BTreeMap<
			ElectionIdentifierOf<<state_chain_runtime::Runtime as pallet_cf_elections::Config<EthereumInstance>>::ElectoralSystemRunner>,
			VoteOf<<state_chain_runtime::Runtime as pallet_cf_elections::Config<EthereumInstance>>::ElectoralSystemRunner>,
		>,
	) -> impl std::future::Future<Output = BTreeSet<ElectionIdentifierOf<<state_chain_runtime::Runtime as pallet_cf_elections::Config<EthereumInstance>>::ElectoralSystemRunner>>> + Send + 'static{
		let base_rpc_client = self.base_rpc_client.clone();
		let account_id = self.signed_extrinsic_client.account_id();
		async move {
			base_rpc_client
				.raw_rpc_client
				.cf_ethereum_filter_votes(account_id, proposed_votes.encode(), None)
				.await
				.map_err(anyhow::Error::from)
				.and_then(|electoral_data| {
					<BTreeSet<
						ElectionIdentifierOf<
							<state_chain_runtime::Runtime as pallet_cf_elections::Config<
								EthereumInstance,
							>>::ElectoralSystemRunner,
						>,
					> as Decode>::decode(&mut &electoral_data[..])
					.map_err(Into::into)
				})
				.inspect_err(|error| {
					error!("Failure in filter_votes rpc: '{}'", error);
				})
				.unwrap_or_default()
		}
	}
}

impl<
		RawRpcClient: RawRpcApi + Send + Sync + 'static,
		SignedExtrinsicClient: SignedExtrinsicApi + Send + Sync + 'static,
	> ElectoralApi<()> for StateChainClient<SignedExtrinsicClient, BaseRpcClient<RawRpcClient>>
{
	fn electoral_data(
		&self,
		block: BlockInfo,
	) -> impl std::future::Future<Output = Option<ElectoralDataFor<state_chain_runtime::Runtime, ()>>>
	       + Send
	       + 'static {
		let base_rpc_client = self.base_rpc_client.clone();
		let account_id = self.signed_extrinsic_client.account_id();
		async move {
			base_rpc_client
				.raw_rpc_client
				.cf_generic_electoral_data(account_id, Some(block.hash))
				.await
				.map_err(anyhow::Error::from)
				.and_then(|electoral_data| {
					<Option<ElectoralDataFor<state_chain_runtime::Runtime, ()>> as Decode>::decode(
						&mut &electoral_data[..],
					)
					.map_err(Into::into)
				})
				.inspect_err(|error| {
					error!("Failure in electoral_data rpc: '{}'", error);
				})
				.ok()
				.flatten()
		}
	}

	fn filter_votes(
		&self,
		proposed_votes: BTreeMap<
			ElectionIdentifierOf<<state_chain_runtime::Runtime as pallet_cf_elections::Config<()>>::ElectoralSystemRunner>,
			VoteOf<<state_chain_runtime::Runtime as pallet_cf_elections::Config<()>>::ElectoralSystemRunner>,
			>
	) -> impl std::future::Future<Output =
	BTreeSet<ElectionIdentifierOf<<state_chain_runtime::Runtime as pallet_cf_elections::Config<()>>::ElectoralSystemRunner>>> + Send + 'static{
		let base_rpc_client = self.base_rpc_client.clone();
		let account_id = self.signed_extrinsic_client.account_id();
		async move {
			base_rpc_client
				.raw_rpc_client
				.cf_generic_filter_votes(account_id, proposed_votes.encode(), None)
				.await
				.map_err(anyhow::Error::from)
				.and_then(|electoral_data| {
					<BTreeSet<
						ElectionIdentifierOf<
							<state_chain_runtime::Runtime as pallet_cf_elections::Config<
								(),
							>>::ElectoralSystemRunner,
						>,
					> as Decode>::decode(&mut &electoral_data[..])
					.map_err(Into::into)
				})
				.inspect_err(|error| {
					error!("Failure in filter_votes rpc: '{}'", error);
				})
				.unwrap_or_default()
		}
	}
}

impl<
		RawRpcClient: RawRpcApi + Send + Sync + 'static,
		SignedExtrinsicClient: SignedExtrinsicApi + Send + Sync + 'static,
	> ElectoralApi<Instance4>
	for StateChainClient<SignedExtrinsicClient, BaseRpcClient<RawRpcClient>>
{
	fn electoral_data(
		&self,
		block: BlockInfo,
	) -> impl std::future::Future<
		Output = Option<ElectoralDataFor<state_chain_runtime::Runtime, ArbitrumInstance>>,
	> + Send
	       + 'static {
		let base_rpc_client = self.base_rpc_client.clone();
		let account_id = self.signed_extrinsic_client.account_id();
		async move {
			base_rpc_client
				.raw_rpc_client
				.cf_arbitrum_electoral_data(account_id, Some(block.hash))
				.await
				.map_err(anyhow::Error::from)
				.and_then(|electoral_data| <Option<ElectoralDataFor<state_chain_runtime::Runtime, ArbitrumInstance>> as Decode>::decode(&mut &electoral_data[..]).map_err(Into::into))
				.inspect_err(|error| {
					error!("Failure in electoral_data rpc: '{}'", error);
				})
				.ok()
				.flatten()
		}
	}

	fn filter_votes(
		&self,
		proposed_votes: BTreeMap<
			ElectionIdentifierOf<<state_chain_runtime::Runtime as pallet_cf_elections::Config<ArbitrumInstance>>::ElectoralSystemRunner>,
			VoteOf<<state_chain_runtime::Runtime as pallet_cf_elections::Config<ArbitrumInstance>>::ElectoralSystemRunner>,
		>,
	) -> impl std::future::Future<Output = BTreeSet<ElectionIdentifierOf<<state_chain_runtime::Runtime as pallet_cf_elections::Config<ArbitrumInstance>>::ElectoralSystemRunner>>> + Send + 'static{
		let base_rpc_client = self.base_rpc_client.clone();
		let account_id = self.signed_extrinsic_client.account_id();
		async move {
			base_rpc_client
				.raw_rpc_client
				.cf_arbitrum_filter_votes(account_id, proposed_votes.encode(), None)
				.await
				.map_err(anyhow::Error::from)
				.and_then(|electoral_data| {
					<BTreeSet<
						ElectionIdentifierOf<
							<state_chain_runtime::Runtime as pallet_cf_elections::Config<
								ArbitrumInstance,
							>>::ElectoralSystemRunner,
						>,
					> as Decode>::decode(&mut &electoral_data[..])
					.map_err(Into::into)
				})
				.inspect_err(|error| {
					error!("Failure in filter_votes rpc: '{}'", error);
				})
				.unwrap_or_default()
		}
	}
}
