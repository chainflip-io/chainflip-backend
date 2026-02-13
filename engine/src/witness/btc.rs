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
pub mod source;
pub mod vault_swaps;

use crate::{
	btc::rpc::{BtcRpcApi, VerboseTransaction},
	witness::{
		btc::fees::predict_fees,
		common::{
			block_height_witnesser::witness_headers,
			traits::{WitnessClient, WitnessClientForBlockData},
		},
	},
};
use bitcoin::{hashes::Hash, BlockHash};
use cf_chains::{
	btc::{self, deposit_address::DepositAddress, BlockNumber, Hash as H256, CHANGE_ADDRESS_SALT},
	Bitcoin, DepositChannel,
};
use cf_primitives::EpochIndex;
use futures_core::Future;

use cf_utilities::task_scope::{self, Scope};
use futures::FutureExt;
use pallet_cf_broadcast::TransactionConfirmation;
use pallet_cf_elections::{
	electoral_system::ElectoralSystemTypes,
	electoral_systems::{
		block_height_witnesser::{primitives::Header, ChainBlockHashOf, ChainTypes},
		block_witnesser::state_machine::{BWElectionProperties, EngineElectionType},
	},
	VoteOf,
};
use pallet_cf_ingress_egress::{DepositWitness, VaultDepositWitness};
use sp_runtime::AccountId32;
use state_chain_runtime::{
	chainflip::witnessing::bitcoin_elections::{
		BitcoinBlockHeightWitnesserES, BitcoinChain, BitcoinDepositChannelWitnessingES,
		BitcoinElectoralSystemRunner, BitcoinLiveness, BITCOIN_MAINNET_SAFETY_BUFFER,
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

use state_chain_runtime::chainflip::witnessing::bitcoin_elections::{
	BitcoinEgressWitnessingES, BitcoinFeeTracking, BitcoinVaultDepositWitnessingES,
};
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
impl
	WitnessClientForBlockData<
		BitcoinChain,
		Vec<DepositChannel<Bitcoin>>,
		Vec<DepositWitness<Bitcoin>>,
	> for BtcCachingClient
{
	async fn block_data_from_query(
		&self,
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
impl
	WitnessClientForBlockData<
		BitcoinChain,
		Vec<PrivateBitcoinVaultChannel>,
		Vec<VaultDepositWitness<Runtime, BitcoinInstance>>,
	> for BtcCachingClient
{
	async fn block_data_from_query(
		&self,
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
impl
	WitnessClientForBlockData<
		BitcoinChain,
		Vec<H256>,
		Vec<TransactionConfirmation<Runtime, BitcoinInstance>>,
	> for BtcCachingClient
{
	async fn block_data_from_query(
		&self,
		tx_hashes: &Vec<H256>,
		block_hash: &Self::BlockQuery,
	) -> Result<Vec<TransactionConfirmation<Runtime, BitcoinInstance>>> {
		let block = self.block(*block_hash).await?;
		let witnesses = egress_witnessing(&block.txdata, tx_hashes.clone());
		Ok(witnesses)
	}
}

async fn query_election_block<C: ChainTypes>(
	client: &BtcCachingClient,
	block_height: btc::BlockNumber,
	election_type: EngineElectionType<C>,
) -> Result<Option<(Vec<VerboseTransaction>, Option<btc::Hash>)>>
where
	ChainBlockHashOf<C>: AsRef<[u8]>,
{
	match election_type {
		EngineElectionType::ByHash(hash) => {
			let block =
				client.block(bitcoin::BlockHash::from_slice(hash.as_ref()).unwrap()).await?;
			Ok(Some((block.txdata, None)))
		},
		EngineElectionType::BlockHeight { submit_hash } => {
			// check whether a block exists with the given height
			if submit_hash {
				let block_hash = client.best_block_hash().await?;
				let best_block_header = client.block_header(block_hash).await?;
				if best_block_header.height < block_height {
					return Ok(None)
				}
			}

			let block_hash = client.block_hash(block_height).await?;
			let block = client.block(block_hash).await?;
			Ok(Some((
				block.txdata,
				if submit_hash { Some(block.header.hash.to_byte_array().into()) } else { None },
			)))
		},
	}
}

#[derive(Clone)]
pub struct BitcoinDepositChannelWitnessingVoter {
	client: BtcCachingClient,
}

#[async_trait::async_trait]
impl VoterApi<BitcoinDepositChannelWitnessingES> for BitcoinDepositChannelWitnessingVoter {
	async fn vote(
		&self,
		_settings: <BitcoinDepositChannelWitnessingES as ElectoralSystemTypes>::ElectoralSettings,
		properties: <BitcoinDepositChannelWitnessingES as ElectoralSystemTypes>::ElectionProperties,
	) -> Result<Option<VoteOf<BitcoinDepositChannelWitnessingES>>, anyhow::Error> {
		let BWElectionProperties {
			block_height, properties: deposit_addresses, election_type, ..
		} = properties;

		let Some((txs, response_block_hash)) =
			query_election_block(&self.client, block_height, election_type).await?
		else {
			return Ok(None)
		};

		let deposit_addresses = map_script_addresses(deposit_addresses);

		let witnesses = deposit_witnesses(&txs, &deposit_addresses);

		Ok(Some((witnesses, response_block_hash)))
	}
}

#[derive(Clone)]
pub struct BitcoinVaultDepositWitnessingVoter {
	client: BtcCachingClient,
}

#[async_trait::async_trait]
impl VoterApi<BitcoinVaultDepositWitnessingES> for BitcoinVaultDepositWitnessingVoter {
	async fn vote(
		&self,
		_settings: <BitcoinVaultDepositWitnessingES as ElectoralSystemTypes>::ElectoralSettings,
		properties: <BitcoinVaultDepositWitnessingES as ElectoralSystemTypes>::ElectionProperties,
	) -> Result<Option<VoteOf<BitcoinVaultDepositWitnessingES>>, anyhow::Error> {
		let BWElectionProperties { block_height, properties: vaults, election_type, .. } =
			properties;

		let Some((txs, response_block_hash)) =
			query_election_block(&self.client, block_height, election_type).await?
		else {
			return Ok(None)
		};

		let witnesses = vault_deposits(&txs, &vaults);
		Ok(Some((witnesses, response_block_hash)))
	}
}

#[derive(Clone)]
pub struct BitcoinEgressWitnessingVoter {
	client: BtcCachingClient,
}

#[async_trait::async_trait]
impl VoterApi<BitcoinEgressWitnessingES> for BitcoinEgressWitnessingVoter {
	async fn vote(
		&self,
		_settings: <BitcoinEgressWitnessingES as ElectoralSystemTypes>::ElectoralSettings,
		properties: <BitcoinEgressWitnessingES as ElectoralSystemTypes>::ElectionProperties,
	) -> Result<Option<VoteOf<BitcoinEgressWitnessingES>>, anyhow::Error> {
		let BWElectionProperties { block_height, properties: tx_hashes, election_type, .. } =
			properties;

		let Some((txs, response_block_hash)) =
			query_election_block(&self.client, block_height, election_type).await?
		else {
			return Ok(None)
		};

		let witnesses = egress_witnessing(&txs, tx_hashes);
		Ok(Some((witnesses, response_block_hash)))
	}
}

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
						BitcoinDepositChannelWitnessingVoter { client: client.clone() },
						BitcoinVaultDepositWitnessingVoter { client: client.clone() },
						BitcoinEgressWitnessingVoter { client: client.clone() },
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

use super::common::epoch_source::Vault;

pub async fn process_egress<ProcessCall, ProcessingFut, ExtraInfo, ExtraHistoricInfo>(
	epoch: Vault<cf_chains::Bitcoin, ExtraInfo, ExtraHistoricInfo>,
	header: super::common::chain_source::Header<
		u64,
		BlockHash,
		(Vec<VerboseTransaction>, Vec<(btc::Hash, BlockNumber)>),
	>,
	process_call: ProcessCall,
) where
	ProcessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> ProcessingFut
		+ Send
		+ Sync
		+ Clone
		+ 'static,
	ProcessingFut: Future<Output = ()> + Send + 'static,
{
	let (txs, monitored_tx_hashes) = header.data;

	let monitored_tx_hashes = monitored_tx_hashes.iter().map(|(tx_hash, _)| tx_hash);

	for (tx_hash, tx) in success_witnesses(monitored_tx_hashes, txs) {
		process_call(
			state_chain_runtime::RuntimeCall::BitcoinBroadcaster(
				pallet_cf_broadcast::Call::transaction_succeeded {
					tx_out_id: tx_hash,
					signer_id: DepositAddress::new(epoch.info.0.current, CHANGE_ADDRESS_SALT)
						.script_pubkey(),
					tx_fee: tx.fee.unwrap_or_default().to_sat(),
					tx_metadata: (),
					transaction_ref: tx_hash,
				},
			),
			epoch.index,
		)
		.await;
	}
}

fn success_witnesses<'a>(
	monitored_tx_hashes: impl Iterator<Item = &'a btc::Hash> + Clone,
	txs: Vec<VerboseTransaction>,
) -> Vec<(btc::Hash, VerboseTransaction)> {
	let mut successful_witnesses = Vec::new();

	for tx in txs {
		let mut monitored = monitored_tx_hashes.clone();
		let tx_hash = tx.txid.to_byte_array().into();

		if monitored.any(|&monitored_hash| monitored_hash == tx_hash) {
			successful_witnesses.push((tx_hash, tx));
		}
	}
	successful_witnesses
}

#[cfg(test)]
mod tests {

	use bitcoin::Amount;

	use super::*;
	use crate::witness::btc::deposits::tests::{fake_transaction, fake_verbose_vouts};

	#[test]
	fn witnesses_tx_hash_successfully() {
		const FEE_0: u64 = 1;
		const FEE_1: u64 = 111;
		const FEE_2: u64 = 222;
		const FEE_3: u64 = 333;
		let txs = vec![
			fake_transaction(vec![], Some(Amount::from_sat(FEE_0))),
			fake_transaction(
				fake_verbose_vouts(vec![(2324, &DepositAddress::new([0; 32], 0))]),
				Some(Amount::from_sat(FEE_1)),
			),
			fake_transaction(
				fake_verbose_vouts(vec![(232232, &DepositAddress::new([32; 32], 0))]),
				Some(Amount::from_sat(FEE_2)),
			),
			fake_transaction(
				fake_verbose_vouts(vec![(232232, &DepositAddress::new([32; 32], 0))]),
				Some(Amount::from_sat(FEE_3)),
			),
		];

		let tx_hashes = txs.iter().map(|tx| tx.txid.to_byte_array().into()).collect::<Vec<_>>();

		// we're not monitoring for index 2, and they're out of order.
		let monitored_hashes = [tx_hashes[3], tx_hashes[0], tx_hashes[1]];

		let sorted_monitored_hashes = vec![tx_hashes[0], tx_hashes[1], tx_hashes[3]];

		let (success_witness_hashes, txs): (Vec<_>, Vec<_>) =
			success_witnesses(monitored_hashes.iter(), txs).into_iter().unzip();
		assert_eq!(sorted_monitored_hashes, success_witness_hashes);
		assert_eq!(txs[0].fee.unwrap().to_sat(), FEE_0);
		assert_eq!(txs[1].fee.unwrap().to_sat(), FEE_1);
		// we weren't monitoring for 2, so the last fee should be FEE_3.
		assert_eq!(txs[2].fee.unwrap().to_sat(), FEE_3);
	}
}
