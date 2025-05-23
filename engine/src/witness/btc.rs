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
pub mod source;
pub mod vault_swaps;

use crate::btc::rpc::{BtcRpcApi, VerboseTransaction};
use bitcoin::{hashes::Hash, BlockHash};
use cf_chains::btc::{
	self, deposit_address::DepositAddress, BlockNumber, Hash as H256, CHANGE_ADDRESS_SALT,
};
use cf_primitives::EpochIndex;
use futures_core::Future;

use cf_utilities::task_scope::{self, Scope};
use futures::FutureExt;
use pallet_cf_elections::{
	electoral_system::ElectoralSystemTypes,
	electoral_systems::{
		block_height_tracking::{
			primitives::{Header, NonemptyContinuousHeaders},
			ChainTypes, HeightWitnesserProperties,
		},
		block_witnesser::state_machine::{BWElectionProperties, BWElectionType},
	},
	VoteOf,
};
use sp_core::bounded::alloc::collections::VecDeque;
use state_chain_runtime::{
	chainflip::{
		bitcoin_elections::{
			BitcoinBlockHeightTracking, BitcoinBlockHeightTrackingES, BitcoinChain,
			BitcoinDepositChannelWitnessingES, BitcoinElectoralSystemRunner, BitcoinLiveness,
		},
		elections::TypesFor,
	},
	BitcoinInstance,
};

use crate::{
	btc::rpc::BlockHeader,
	elections::voter_api::{CompositeVoter, VoterApi},
	state_chain_observer::client::{
		chain_api::ChainApi, electoral_api::ElectoralApi,
		extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi,
	},
	witness::btc::deposits::{deposit_witnesses, map_script_addresses},
};
use anyhow::Result;

use state_chain_runtime::chainflip::bitcoin_elections::{
	BitcoinEgressWitnessingES, BitcoinFeeTracking, BitcoinVaultDepositWitnessingES,
};
use std::sync::Arc;

use crate::{
	btc::cached_rpc::BtcCachingClient,
	witness::btc::deposits::{egress_witnessing, vault_deposits},
};

#[derive(Clone)]
pub struct BitcoinBlockHeightTrackingVoter {
	client: BtcCachingClient,
}

#[async_trait::async_trait]
impl VoterApi<BitcoinBlockHeightTrackingES> for BitcoinBlockHeightTrackingVoter {
	async fn vote(
		&self,
		_settings: <BitcoinBlockHeightTrackingES as ElectoralSystemTypes>::ElectoralSettings,
		// We could use 0 as a special case (to avoid requiring an Option)
		properties: <BitcoinBlockHeightTrackingES as ElectoralSystemTypes>::ElectionProperties,
	) -> std::result::Result<Option<VoteOf<BitcoinBlockHeightTrackingES>>, anyhow::Error> {
		tracing::info!("Block height tracking called properties: {:?}", properties);
		let HeightWitnesserProperties { witness_from_index: latest_block_height } = properties;

		let mut headers = VecDeque::new();

		let header_from_btc_header = |header: BlockHeader| -> anyhow::Result<Header<BitcoinChain>> {
			Ok(Header {
				block_height: header.height,
				hash: header.hash.to_byte_array().into(),
				parent_hash: header
					.previous_block_hash
					.ok_or_else(|| anyhow::anyhow!("No parent hash"))?
					.to_byte_array()
					.into(),
			})
		};

		let get_header = |index: BlockNumber| {
			async move {
				let block_hash = self.client.block_hash(index).await?;
				let header = self.client.block_header(block_hash).await?;
				// tracing::info!("bht: Voting for block height tracking: {:?}", header.height);
				// Order from lowest to highest block index.
				header_from_btc_header(header)
			}
		};

		let best_block_hash = self.client.best_block_hash().await?;
		let best_block_header = self.client.block_header(best_block_hash).await?;
		if best_block_hash != best_block_header.hash {
			return Err(anyhow::anyhow!("best_block_hash do not match best header hash"))
		}
		let best_block_header = header_from_btc_header(best_block_header)?;
		if best_block_header.block_height <= latest_block_height {
			tracing::info!("btc: no new blocks found since best block height is {} for witness_from={latest_block_height}", best_block_header.block_height);
			return Ok(None)
		} else {
			let witness_from_index = if latest_block_height == 0 {
				tracing::info!(
					"bht: election_property=0, best_block_height={}, submitting last 6 blocks.",
					best_block_header.block_height
				);
				best_block_header
					.block_height
					.saturating_sub(BitcoinChain::SAFETY_BUFFER as u64)
			} else {
				latest_block_height
			};

			// fetch the headers we haven't got yet
			for index in witness_from_index..best_block_header.block_height {
				// let header = self.client.block_header(index).await?;
				// tracing::info!("bht: Voting for block height tracking: {:?}", header.height);
				// Order from lowest to highest block index.
				headers.push_back(get_header(index).await?);
			}

			headers.push_back(best_block_header);
			tracing::info!(
				"bht: Voting for block height tracking: {:?}",
				headers.iter().map(|h| h.block_height)
			);

			// We should have a chain of hashees.
			if headers.iter().zip(headers.iter().skip(1)).all(|(a, b)| a.hash == b.parent_hash) {
				tracing::info!(
					"bht: Submitting vote for (witness_from={latest_block_height})with {} headers",
					headers.len()
				);
				Ok(Some(NonemptyContinuousHeaders { headers }))
			} else {
				Err(anyhow::anyhow!("bht: Headers do not form a chain"))
			}
		}
	}
}

async fn query_election_block<T: ChainTypes>(
	client: &BtcCachingClient,
	block_height: btc::BlockNumber,
	election_type: BWElectionType<T>,
) -> Result<(Vec<VerboseTransaction>, Option<btc::Hash>)>
where
	T::ChainBlockHash: AsRef<[u8]>,
{
	match election_type {
		BWElectionType::Optimistic => {
			let block_hash = client.block_hash(block_height).await?;
			let block = client.block(block_hash).await?;
			Ok((block.txdata, Some(block.header.hash.to_byte_array().into())))
		},
		BWElectionType::ByHash(hash) => {
			let block =
				client.block(bitcoin::BlockHash::from_slice(hash.as_ref()).unwrap()).await?;
			Ok((block.txdata, None))
		},
		BWElectionType::SafeBlockHeight => {
			let block_hash = client.block_hash(block_height).await?;
			let block = client.block(block_hash).await?;
			Ok((block.txdata, None))
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

		let (txs, response_block_hash) =
			query_election_block(&self.client, block_height, election_type).await?;

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

		let (txs, response_block_hash) =
			query_election_block(&self.client, block_height, election_type).await?;

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

		let (txs, response_block_hash) =
			query_election_block(&self.client, block_height, election_type).await?;

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
		_settings: <BitcoinFeeTracking as ElectoralSystemTypes>::ElectoralSettings,
		_properties: <BitcoinFeeTracking as ElectoralSystemTypes>::ElectionProperties,
	) -> Result<Option<VoteOf<BitcoinFeeTracking>>, anyhow::Error> {
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
	tracing::info!("Starting BTC witness");
	scope.spawn(async move {
		task_scope::task_scope(|scope| {
			async {
				crate::elections::Voter::new(
					scope,
					state_chain_client,
					CompositeVoter::<BitcoinElectoralSystemRunner, _>::new((
						BitcoinBlockHeightTrackingVoter { client: client.clone() },
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
