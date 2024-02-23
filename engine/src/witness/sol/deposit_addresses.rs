use std::collections::HashSet;

use futures::{
	stream::{Stream, StreamExt},
	TryStreamExt,
};
use pallet_cf_ingress_egress::DepositWitness;
use state_chain_runtime::SolanaInstance;
use tokio_stream::wrappers::IntervalStream;

use cf_chains::{assets::sol::Asset, sol::SolAddress, Solana};
use cf_primitives::ChannelId;

use crate::state_chain_observer::client::{
	chain_api::ChainApi, extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi,
};

use std::{
	collections::HashMap,
	future::Future,
	sync::{atomic::AtomicBool, Arc},
};

use anyhow::Result;
use futures::FutureExt;

use cf_primitives::EpochIndex;
use sol_prim::SlotNumber;
use sol_rpc::traits::CallApi as SolanaApi;
use sol_watch::{
	address_transactions_stream::AddressSignatures, deduplicate_stream::DeduplicateStreamExt,
	ensure_balance_continuity::EnsureBalanceContinuityStreamExt,
	fetch_balance::FetchBalancesStreamExt,
};

use super::{
	SC_BLOCK_TIME, SOLANA_SIGNATURES_FOR_TRANSACTION_PAGE_SIZE,
	SOLANA_SIGNATURES_FOR_TRANSACTION_POLL_INTERVAL,
};

#[derive(Debug, Clone)]
pub struct DepositAddressesUpdate<
	Added = (ChannelId, SolAddress, SlotNumber, SlotNumber),
	Removed = (ChannelId, SolAddress),
> {
	pub added: Vec<Added>,
	pub removed: Vec<Removed>,
}

pub fn deposit_addresses_updates<StateChainClient>(
	state_chain_client: &StateChainClient,
) -> impl Stream<Item = DepositAddressesUpdate> + '_
where
	StateChainClient: StorageApi + ChainApi + SignedExtrinsicApi + 'static + Send + Sync,
{
	IntervalStream::new(tokio::time::interval(SC_BLOCK_TIME))
		.then(|_| {
			let sc_latest_finalized_block = state_chain_client.latest_finalized_block();
			state_chain_client.storage_map_values::<pallet_cf_ingress_egress::DepositChannelLookup<
				state_chain_runtime::Runtime,
				SolanaInstance,
			>>(sc_latest_finalized_block.hash)
		})
		.filter_map(|result| async move {
			match result {
				Ok(deposit_addresses) => Some(deposit_addresses),
				Err(reason) => {
					tracing::warn!("Error fetching deposit-addresses: {}", reason);
					None
				},
			}
		})
		.map(|current_vec| {
			current_vec
				.into_iter()
				.map(|entry| (entry.deposit_channel.channel_id, entry))
				.collect::<HashMap<_, _>>()
		})
		.scan(HashMap::<ChannelId, _>::new(), |current_map, new_map| {
			let prev_map = std::mem::replace(current_map, new_map);

			let prev_keys = prev_map.keys().collect::<HashSet<_>>();
			let current_keys = current_map.keys().collect::<HashSet<_>>();

			let removed_keys = prev_keys.difference(&current_keys);
			let added_keys = current_keys.difference(&prev_keys);

			let added = added_keys
				.flat_map(|k| current_map.get(k))
				.map(|e| {
					(
						e.deposit_channel.channel_id,
						e.deposit_channel.address,
						e.opened_at,
						e.expires_at,
					)
				})
				.collect();

			let removed = removed_keys
				.flat_map(|k| prev_map.get(k))
				.map(|e| (e.deposit_channel.channel_id, e.deposit_channel.address))
				.collect();

			async move { Some(DepositAddressesUpdate { added, removed }) }
		})
	// .then(|update| populate_update_with_channel_state(state_chain_client, update))
}

pub async fn track_deposit_addresses<SolanaClient, StateChainClient, ProcessCall, ProcessingFut>(
	sol_client: Arc<SolanaClient>,
	process_call: ProcessCall,
	state_chain_client: Arc<StateChainClient>,
) -> Result<()>
where
	SolanaClient: SolanaApi + Send + Sync + 'static,
	StateChainClient: StorageApi + ChainApi + SignedExtrinsicApi + 'static + Send + Sync,
	ProcessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> ProcessingFut
		+ Send
		+ Sync
		+ Clone
		+ 'static,
	ProcessingFut: Future<Output = ()> + Send + 'static,
{
	// std::mem::drop(state_chain_stream);

	utilities::task_scope::task_scope(move |scope| {
		async move {
			let deposit_addresses_updates = deposit_addresses_updates(state_chain_client.as_ref());
			let mut deposit_addresses_updates = std::pin::pin!(deposit_addresses_updates);

			let mut deposit_processor_kill_switches = HashMap::new();

			while let Some(update) = deposit_addresses_updates.next().await {
				for (channel_id, address, opened_at, expires_at) in update.added {
					tracing::info!(
						"starting up a deposit-address tracking for #{}: {} [{}..{}]",
						channel_id,
						address,
						opened_at,
						expires_at,
					);

					let kill_switch = Arc::new(AtomicBool::default());
					deposit_processor_kill_switches.insert(channel_id, Arc::clone(&kill_switch));

					let running = track_single_deposit_address(
						Arc::clone(&sol_client),
						channel_id,
						address,
						opened_at,
						expires_at,
						kill_switch,
						process_call.clone(),
					);
					scope.spawn(running);
				}
				for (channel_id, address) in update.removed {
					tracing::info!(
						"shutting down a deposit-address tracking for #{}: {}",
						channel_id,
						address
					);

					if let Some(kill_switch) = deposit_processor_kill_switches.remove(&channel_id) {
						kill_switch.store(false, std::sync::atomic::Ordering::Relaxed);
					} else {
						tracing::warn!("Could not find a kill-switch for channel #{}", channel_id);
					}
				}
			}

			Ok(())
		}
		.boxed()
	})
	.await
}

async fn track_single_deposit_address<SolanaClient, ProcessCall, ProcessingFut>(
	sol_client: Arc<SolanaClient>,
	channel_id: ChannelId,
	address: SolAddress,
	opened_at: SlotNumber,
	_expires_at: SlotNumber,
	kill_switch: Arc<AtomicBool>,
	process_call: ProcessCall,
) -> Result<()>
where
	SolanaClient: SolanaApi + Send + Sync + 'static,
	ProcessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> ProcessingFut
		+ Send
		+ Sync
		+ Clone
		+ 'static,
	ProcessingFut: Future<Output = ()> + Send + 'static,
{
	AddressSignatures::new(Arc::clone(&sol_client), address, kill_switch)
		.max_page_size(SOLANA_SIGNATURES_FOR_TRANSACTION_PAGE_SIZE)
		.poll_interval(SOLANA_SIGNATURES_FOR_TRANSACTION_POLL_INTERVAL)
		// // TODO: find a way to start from where we may have left
		// .after_transaction(last_known_transaction)
		.starting_with_slot(opened_at)
		.into_stream()
		.deduplicate(
			SOLANA_SIGNATURES_FOR_TRANSACTION_PAGE_SIZE * 2,
			|r| r.as_ref().ok().copied(),
			|tx_id, _| {
				tracing::debug!("AddressSignatures has returned a duplicate tx-id: {}", tx_id);
			},
		)
		.fetch_balances(Arc::clone(&sol_client), address)
		.map_err(anyhow::Error::from)
		.ensure_balance_continuity(SOLANA_SIGNATURES_FOR_TRANSACTION_PAGE_SIZE)
		.try_for_each(|balance| {
			let process_call = &process_call;
			async move {
				if let Some(deposited_amount) = balance.deposited() {
					tracing::info!(
						"  deposit-address #{}: +{} lamports; [addr: {}; tx: {}]",
						channel_id,
						deposited_amount,
						address,
						balance.signature,
					);

					let deposit_witness = DepositWitness::<Solana> {
						deposit_address: address,
						asset: Asset::Sol,
						amount: deposited_amount,
						deposit_details: (),
					};

					process_call(
						pallet_cf_ingress_egress::Call::<_, SolanaInstance>::process_deposits {
							deposit_witnesses: vec![deposit_witness],
							block_height: balance.slot,
						}
						.into(),
						1, // FIXME: need an epoch here
					)
					.await;
				}
				Ok(())
			}
		})
		.await?;
	Ok(())
}
