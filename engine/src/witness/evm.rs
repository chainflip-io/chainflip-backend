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

pub mod contract_common;
pub mod erc20_deposits;
pub mod evm_deposits;
pub mod key_manager;
pub mod source;
pub mod vault;

use anyhow::Result;
use derive_where::derive_where;
use ethers::types::{Transaction, TransactionReceipt};
use itertools::Itertools;
use sp_runtime::AccountId32;
use state_chain_runtime::chainflip::witnessing::pallet_hooks::{
	self, EvmKeyManagerEvent, EvmVaultContractEvent,
};
use std::{collections::HashMap, fmt::Debug};

use cf_chains::{evm::EvmChain, Chain, DepositChannel};
use pallet_cf_elections::electoral_systems::block_height_witnesser::ChainTypes;
use pallet_cf_ingress_egress::DepositWitness;
use sp_core::{H160, H256};

use crate::{
	evm::{
		cached_rpc::{EvmCachingClient, EvmRetryRpcApiWithResult},
		event::{Event, EvmEventSource},
		rpc::{address_checker::AddressState, EvmRpcSigningClient},
	},
	witness::{
		common::traits::{WitnessClient, WitnessClientForBlockData},
		evm::{
			contract_common::witness_deposit_channels_generic,
			erc20_deposits::Erc20Events,
			key_manager::{handle_key_manager_events, KeyManagerEvents},
			vault::{handle_vault_events, VaultEvents},
		},
	},
};

pub trait EvmBlockQuery {
	/// For deposit channel witnessing prior to contract deployment we need access to the block
	/// height, which is used to compare address states at previous block and current block.
	fn get_lowest_block_height_of_query(&self) -> u64;
}

#[async_trait::async_trait]
pub trait EvmEventClient<Chain: ChainTypes>:
	WitnessClient<Chain, BlockQuery: EvmBlockQuery>
{
	async fn events_from_block_query<Data: Debug>(
		&self,
		event_source: &EvmEventSource<Data>,
		query: Self::BlockQuery,
	) -> Result<Vec<Event<Data>>>;
}

#[async_trait::async_trait]
pub trait EvmAddressStateClient<Chain: ChainTypes>:
	WitnessClient<Chain, BlockQuery: EvmBlockQuery>
{
	async fn address_states(
		&self,
		address_checker_address: H160,
		query: Self::BlockQuery,
		addresses: Vec<H160>,
	) -> Result<HashMap<H160, (AddressState, AddressState)>, anyhow::Error>;
}

#[async_trait::async_trait]
pub trait EvmTransactionClient {
	async fn transaction_receipt(&self, tx_hash: H256) -> Result<TransactionReceipt>;
	async fn get_transaction(&self, tx_hash: H256) -> Result<Transaction>;
}

#[derive_where(Clone; )]
pub struct EvmVoter<CT: ChainTypes, BlockQuery> {
	pub client: EvmCachingClient<EvmRpcSigningClient>,
	pub _phantom: std::marker::PhantomData<(CT, BlockQuery)>,
}

impl<CT: ChainTypes, BlockQuery> EvmVoter<CT, BlockQuery> {
	pub fn new(client: EvmCachingClient<EvmRpcSigningClient>) -> Self {
		Self { client, _phantom: Default::default() }
	}
}

#[async_trait::async_trait]
impl<CT: ChainTypes + Sync + Send, BlockQuery: Sync + Send> EvmTransactionClient
	for EvmVoter<CT, BlockQuery>
{
	async fn transaction_receipt(&self, tx_hash: H256) -> Result<TransactionReceipt> {
		self.client.transaction_receipt(tx_hash).await
	}

	async fn get_transaction(&self, tx_hash: H256) -> Result<Transaction> {
		self.client.get_transaction(tx_hash).await
	}
}

// ----- deposit channel querying -----

#[derive(Clone)]
pub struct EvmDepositChannelWitnessingConfig<C: Chain> {
	pub address_checker_address: H160,
	pub vault_contract: EvmEventSource<VaultEvents>,
	pub supported_assets: HashMap<C::ChainAsset, EvmEventSource<Erc20Events>>,
}

#[async_trait::async_trait]
impl<
		CT: ChainTypes<ChainBlockHash = H256>,
		C: EvmChain<ChainAsset: std::hash::Hash>,
		BlockQuery,
	> WitnessClientForBlockData<CT, Vec<DepositWitness<C>>> for EvmVoter<CT, BlockQuery>
where
	Self: EvmAddressStateClient<CT> + EvmEventClient<CT>,
{
	type ElectionProperties = Vec<DepositChannel<C>>;
	type Config = EvmDepositChannelWitnessingConfig<C>;
	async fn block_data_from_query(
		&self,
		config: &Self::Config,
		deposit_channels: &Vec<DepositChannel<C>>,
		query: &Self::BlockQuery,
	) -> Result<Vec<DepositWitness<C>>> {
		witness_deposit_channels_generic::<C, CT, Self>(
			&self,
			config,
			query,
			deposit_channels.clone(),
		)
		.await
	}
}

// ----- vault deposit witnessing -----
#[derive_where(Clone; )]
pub struct VaultDepositWitnessingConfig<C: Chain> {
	pub vault: EvmEventSource<VaultEvents>,
	pub supported_assets: HashMap<H160, C::ChainAsset>,
}

#[async_trait::async_trait]
impl<
		CT: ChainTypes,
		T: pallet_cf_ingress_egress::Config<I, TargetChain: EvmChain, AccountId = AccountId32>,
		I: 'static,
		Client: EvmEventClient<CT>,
	> WitnessClientForBlockData<CT, Vec<EvmVaultContractEvent<T, I>>> for Client
{
	type Config = VaultDepositWitnessingConfig<T::TargetChain>;
	async fn block_data_from_query(
		&self,
		config: &Self::Config,
		_properties: &(),
		query: &Self::BlockQuery,
	) -> Result<Vec<EvmVaultContractEvent<T, I>>> {
		let events = self.events_from_block_query(&config.vault, query.clone()).await?;
		let result = handle_vault_events::<T, I>(&config.supported_assets, events, query)?;
		Ok(result.into_iter().sorted().collect())
	}
}

// ----- key manager witnessing -----
#[derive(Clone)]
pub struct EvmKeyManagerWitnessingConfig {
	pub key_manager: EvmEventSource<KeyManagerEvents>,
}

#[async_trait::async_trait]
impl<
		CT: ChainTypes,
		T: pallet_hooks::Config<I, TargetChain: EvmChain, AccountId = AccountId32>,
		I: 'static,
		Client: EvmEventClient<CT> + EvmTransactionClient,
	> WitnessClientForBlockData<CT, Vec<EvmKeyManagerEvent<T, I>>> for Client
{
	type Config = EvmKeyManagerWitnessingConfig;
	async fn block_data_from_query(
		&self,
		config: &Self::Config,
		_properties: &(),
		query: &Self::BlockQuery,
	) -> Result<Vec<EvmKeyManagerEvent<T, I>>> {
		let block_height = query.get_lowest_block_height_of_query();
		let events = self.events_from_block_query(&config.key_manager, query.clone()).await?;
		let result = handle_key_manager_events::<T, I>(self, events, block_height).await?;
		Ok(result.into_iter().sorted().collect())
	}
}
