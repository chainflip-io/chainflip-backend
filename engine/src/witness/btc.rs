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

pub mod deposits;
pub mod fees;
pub mod vault_swaps;

use crate::{
	btc::rpc::BtcRpcApi,
	witness::{
		btc::fees::predict_fees,
		common::{
			block_height_witnesser::witness_headers,
			block_witnesser::GenericBwVoter,
			traits::{WitnessClient, WitnessClientForBlockData},
		},
	},
};
use bitcoin::{hashes::Hash, BlockHash};
use cf_chains::{
	btc::{deposit_address::DepositAddress, Hash as H256},
	Bitcoin, DepositChannel,
};

use cf_utilities::task_scope::{self, Scope};
use futures::FutureExt;
use pallet_cf_broadcast::TransactionConfirmation;
use pallet_cf_elections::{
	electoral_system::ElectoralSystemTypes,
	electoral_systems::block_height_witnesser::{primitives::Header, ChainBlockHashOf},
	VoteOf,
};
use pallet_cf_ingress_egress::{DepositWitness, VaultDepositWitness};
use sp_runtime::AccountId32;
use state_chain_runtime::{
	chainflip::witnessing::bitcoin_elections::{
		BitcoinBlockHeightWitnesserES, BitcoinChain, BitcoinElectoralSystemRunner, BitcoinLiveness,
		BITCOIN_MAINNET_SAFETY_BUFFER,
	},
	BitcoinInstance, Runtime,
};

use crate::{
	elections::voter_api::{CompositeVoter, VoterApi},
	witness::btc::deposits::{deposit_witnesses, map_script_addresses},
};
use anyhow::Result;
use engine_sc_client::{
	chain_api::ChainApi, electoral_api::ElectoralApi, extrinsic_api::signed::SignedExtrinsicApi,
	storage_api::StorageApi,
};

use state_chain_runtime::chainflip::witnessing::bitcoin_elections::BitcoinFeeTracking;
use std::sync::Arc;

use crate::{
	btc::cached_rpc::BtcCachingClient,
	witness::btc::deposits::{egress_witnessing, vault_deposits},
};

#[derive(Clone)]
pub struct BitcoinBlockHeightWitnesserVoter {
	client: BtcCachingClient,
}

#[async_trait::async_trait]
impl WitnessClient<BitcoinChain> for BtcCachingClient {
	type BlockQuery = BlockHash;

	async fn best_block_header(&self) -> Result<Header<BitcoinChain>> {
		let best_hash = self.best_block_hash().await?;
		let best_header = self.block_header(best_hash).await?;
		if best_hash != best_header.hash {
			anyhow::bail!(
				"BTC: best_block_hash {best_hash:?} does not match best header hash {:?}",
				best_header.hash
			);
		}
		Ok(Header {
			block_height: best_header.height,
			hash: best_header.hash.to_byte_array().into(),
			parent_hash: best_header
				.previous_block_hash
				.ok_or_else(|| anyhow::anyhow!("No parent hash"))?
				.to_byte_array()
				.into(),
		})
	}

	async fn best_block_number(&self) -> Result<u64> {
		let best_hash = self.best_block_hash().await?;
		let best_header = self.block_header(best_hash).await?;
		Ok(best_header.height)
	}

	async fn block_header_by_height(&self, height: u64) -> Result<Header<BitcoinChain>> {
		let hash = self.block_hash(height).await?;
		let header = self
			.block_header(bitcoin::BlockHash::from_slice(hash.as_ref()).unwrap())
			.await?;
		Ok(Header {
			block_height: header.height,
			hash: header.hash.to_byte_array().into(),
			parent_hash: header
				.previous_block_hash
				.ok_or_else(|| anyhow::anyhow!("No parent hash"))?
				.to_byte_array()
				.into(),
		})
	}

	async fn block_query_from_hash_and_height(
		&self,
		hash: ChainBlockHashOf<BitcoinChain>,
		_height: u64,
	) -> Result<Self::BlockQuery> {
		Ok(bitcoin::BlockHash::from_slice(hash.as_ref()).unwrap())
	}

	async fn block_query_from_height(&self, height: u64) -> Result<Self::BlockQuery> {
		self.block_hash(height).await
	}

	async fn block_query_and_hash_from_height(
		&self,
		height: u64,
	) -> Result<(Self::BlockQuery, ChainBlockHashOf<BitcoinChain>)> {
		let hash = self.block_hash(height).await?;
		let bhw_hash = hash.to_byte_array().into();
		Ok((hash, bhw_hash))
	}
}

#[async_trait::async_trait]
impl VoterApi<BitcoinBlockHeightWitnesserES> for BitcoinBlockHeightWitnesserVoter {
	async fn vote(
		&self,
		_settings: <BitcoinBlockHeightWitnesserES as ElectoralSystemTypes>::ElectoralSettings,
		properties: <BitcoinBlockHeightWitnesserES as ElectoralSystemTypes>::ElectionProperties,
	) -> std::result::Result<Option<VoteOf<BitcoinBlockHeightWitnesserES>>, anyhow::Error> {
		witness_headers::<BitcoinBlockHeightWitnesserES, _, BitcoinChain>(
			&self.client,
			properties,
			BITCOIN_MAINNET_SAFETY_BUFFER,
			"BTC BHW",
		)
		.await
	}
}

// --- deposit channel witnessing ---
#[async_trait::async_trait]
impl WitnessClientForBlockData<BitcoinChain, Vec<DepositWitness<Bitcoin>>> for BtcCachingClient {
	type ElectionProperties = Vec<DepositChannel<Bitcoin>>;
	async fn block_data_from_query(
		&self,
		_config: &(),
		deposit_addresses: &Vec<DepositChannel<Bitcoin>>,
		block_hash: &Self::BlockQuery,
	) -> Result<Vec<DepositWitness<Bitcoin>>> {
		let block = self.block(*block_hash).await?;
		let deposit_addresses = map_script_addresses(deposit_addresses.clone());
		let witnesses = deposit_witnesses(&block.txdata, &deposit_addresses);
		Ok(witnesses)
	}
}

// --- vault swap witnessing ---
type PrivateBitcoinVaultChannel = (DepositAddress, AccountId32, u64);

#[async_trait::async_trait]
impl WitnessClientForBlockData<BitcoinChain, Vec<VaultDepositWitness<Runtime, BitcoinInstance>>>
	for BtcCachingClient
{
	type ElectionProperties = Vec<PrivateBitcoinVaultChannel>;
	async fn block_data_from_query(
		&self,
		_config: &(),
		vault_channels: &Vec<PrivateBitcoinVaultChannel>,
		block_hash: &Self::BlockQuery,
	) -> Result<Vec<VaultDepositWitness<Runtime, BitcoinInstance>>> {
		let block = self.block(*block_hash).await?;
		let witnesses = vault_deposits(&block.txdata, vault_channels);
		Ok(witnesses)
	}
}

// --- egress witnessing ---
#[async_trait::async_trait]
impl WitnessClientForBlockData<BitcoinChain, Vec<TransactionConfirmation<Runtime, BitcoinInstance>>>
	for BtcCachingClient
{
	type ElectionProperties = Vec<H256>;
	async fn block_data_from_query(
		&self,
		_config: &(),
		tx_hashes: &Vec<H256>,
		block_hash: &Self::BlockQuery,
	) -> Result<Vec<TransactionConfirmation<Runtime, BitcoinInstance>>> {
		let block = self.block(*block_hash).await?;
		let witnesses = egress_witnessing(&block.txdata, tx_hashes.clone());
		Ok(witnesses)
	}
}

// --- fee witnessing ---
#[derive(Clone)]
pub struct BitcoinFeeVoter {
	client: BtcCachingClient,
}

#[async_trait::async_trait]
impl VoterApi<BitcoinFeeTracking> for BitcoinFeeVoter {
	async fn vote(
		&self,
		settings: <BitcoinFeeTracking as ElectoralSystemTypes>::ElectoralSettings,
		_properties: <BitcoinFeeTracking as ElectoralSystemTypes>::ElectionProperties,
	) -> Result<Option<VoteOf<BitcoinFeeTracking>>, anyhow::Error> {
		if settings.tx_sample_count_per_mempool_block > 0 {
			match predict_fees(&self.client, settings.tx_sample_count_per_mempool_block).await {
				Ok(fee) =>
					return Ok(Some(fee + settings.fixed_median_fee_adjustement_sat_per_vkilobyte)),
				Err(err) => {
					tracing::debug!("Could not estimate median mempool fees due to err: {err}. Falling back to native rpc call.");
				},
			}
		}

		if let Some(fee) = self.client.next_block_fee_rate().await? {
			Ok(Some(fee))
		} else {
			let hash = self.client.best_block_hash().await?;
			Ok(Some(self.client.average_block_fee_rate(hash).await?))
		}
	}
}

// --- liveness witnessing ---
#[derive(Clone)]
pub struct BitcoinLivenessVoter {
	client: BtcCachingClient,
}

#[async_trait::async_trait]
impl VoterApi<BitcoinLiveness> for BitcoinLivenessVoter {
	async fn vote(
		&self,
		_settings: <BitcoinLiveness as ElectoralSystemTypes>::ElectoralSettings,
		properties: <BitcoinLiveness as ElectoralSystemTypes>::ElectionProperties,
	) -> Result<Option<VoteOf<BitcoinLiveness>>, anyhow::Error> {
		Ok(Some(H256::from_slice(&self.client.block_hash(properties).await?.to_byte_array())))
	}
}

pub async fn start<StateChainClient>(
	scope: &Scope<'_, anyhow::Error>,
	client: BtcCachingClient,
	state_chain_client: Arc<StateChainClient>,
) -> Result<()>
where
	StateChainClient: StorageApi
		+ ChainApi
		+ SignedExtrinsicApi
		+ ElectoralApi<BitcoinInstance>
		+ 'static
		+ Send
		+ Sync,
{
	tracing::debug!("Starting BTC witness");
	scope.spawn(async move {
		task_scope::task_scope(|scope| {
			async {
				crate::elections::Voter::new(
					scope,
					state_chain_client,
					CompositeVoter::<BitcoinElectoralSystemRunner, _>::new((
						BitcoinBlockHeightWitnesserVoter { client: client.clone() },
						GenericBwVoter::new(client.clone(), ()),
						GenericBwVoter::new(client.clone(), ()),
						GenericBwVoter::new(client.clone(), ()),
						BitcoinFeeVoter { client: client.clone() },
						BitcoinLivenessVoter { client: client.clone() },
					)),
					Some(client.cache_invalidation_senders),
					"Bitcoin",
				)
				.continuously_vote()
				.await;

				Ok(())
			}
			.boxed()
		})
		.await
	});

	Ok(())
}
