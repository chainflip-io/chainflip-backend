use std::{
	collections::{HashMap, HashSet},
	future::Future,
	sync::{atomic::AtomicBool, Arc},
};

use anyhow::Result;
use futures::{
	stream::{self, Stream, StreamExt},
	TryStreamExt,
};
use tokio_stream::wrappers::IntervalStream;

use cf_chains::{assets::sol::Asset, sol::SolAddress, Solana};
use cf_primitives::{ChannelId, EpochIndex};
use pallet_cf_ingress_egress::DepositWitness;
use sol_prim::SlotNumber;
use sol_rpc::traits::CallApi as SolanaApi;
use sol_watch::{
	address_transactions_stream::AddressSignatures,
	deduplicate_stream::DeduplicateStreamExt,
	ensure_balance_continuity::EnsureBalanceContinuityStreamExt,
	fetch_balance::{Balance, FetchBalancesStreamExt},
};
use state_chain_runtime::SolanaInstance;

use crate::{
	state_chain_observer::client::{
		chain_api::ChainApi, extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi,
	},
	witness::{
		common::epoch_source::{Epoch, EpochSource},
		sol::epoch_stream::epoch_stream,
	},
};

use super::{
	zip_with_latest::TryZipLatestExt, SC_BLOCK_TIME, SOLANA_SIGNATURES_FOR_TRANSACTION_PAGE_SIZE,
	SOLANA_SIGNATURES_FOR_TRANSACTION_POLL_INTERVAL,
};

pub async fn track_deposit_addresses<SolanaClient, StateChainClient, ProcessCall, ProcessingFut>(
	epoch_source: EpochSource<(), ()>,
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
	deposit_addresses_updates(state_chain_client.as_ref())
		.scan(HashMap::<ChannelId, Arc<AtomicBool>>::new(), |kill_switches, update| {
			let sol_client = Arc::clone(&sol_client);

			for (channel_id, address) in update.removed {
				tracing::info!(
					"shutting down a deposit-address tracking for #{}: {}",
					channel_id,
					address
				);

				if let Some(kill_switch) = kill_switches.remove(&channel_id) {
					kill_switch.store(false, std::sync::atomic::Ordering::Relaxed);
				} else {
					tracing::warn!("Could not find a kill-switch for channel #{}", channel_id);
				}
			}

			// Scan thinks that we'd be hogging `&mut State` for longer than necessary:
			// it's as if it isn't going to resolve the returned by this closure future before
			// proceeding to the next item. :\ Anyways, effectively we must wrap up all our business
			// with the `&mut State` before doing async stuff, so that the state does not get closed
			// into the future.
			//
			// Hence this seemingly unnecessary intermediate step...
			let added_streams_arguments = update
				.added
				.into_iter()
				.map(|(channel_id, address, opened_at, expires_at)| {
					let epoch_source = epoch_source.clone();
					let sol_client = Arc::clone(&sol_client);
					let kill_switch = Arc::new(AtomicBool::default());

					kill_switches.insert(channel_id, Arc::clone(&kill_switch));
					TrackerArgs {
						channel_id,
						address,
						opened_at,
						expires_at,
						epochs: epoch_source,
						sol_client,
						kill_switch,
					}
				})
				.collect::<Vec<_>>();

			std::future::ready(Some(stream::iter(added_streams_arguments)))
		})
		.flatten()
		.then(
			|TrackerArgs {
			     channel_id,
			     address,
			     opened_at,
			     expires_at,
			     epochs: epoch_source,
			     sol_client,
			     kill_switch,
			 }| async move {
				let epoch_stream = epoch_stream(epoch_source).await;
				TrackerArgs {
					channel_id,
					address,
					opened_at,
					expires_at,
					epochs: epoch_stream,
					sol_client,
					kill_switch,
				}
			},
		)
		.map(single_deposit_address_stream)
		.map(StreamExt::boxed)
		// We can't easily add something like
		// `.flatten_unordered(MAX_CONCURRENT_DEPOSIT_ADDRESS_TRACKERS)`,
		// as in this case we can't guarantee of the upstream,
		// and that leads to inability to find out that any of the running channels is expired.
		.flatten_unordered(None)
		// The failures are inspected and reported above
		.filter_map(|r| async move { r.ok() })
		.for_each(|(channel_id, address, epoch, balance)| {
			let process_call = &process_call;
			async move {
				if let Some(deposited_amount) = balance.deposited() {
					tracing::info!(
						"  witnessing a deposit at #{}: +{} lamports; [addr: {}; tx: {}]",
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
						epoch.index,
					)
					.await;
				}
			}
		})
		.await;

	Ok(())
}

#[derive(Debug, Clone)]
struct DepositAddressesUpdate<
	Added = (ChannelId, SolAddress, SlotNumber, SlotNumber),
	Removed = (ChannelId, SolAddress),
> {
	pub added: Vec<Added>,
	pub removed: Vec<Removed>,
}

fn deposit_addresses_updates<StateChainClient>(
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
}

struct TrackerArgs<SolanaClient, Ep> {
	channel_id: ChannelId,
	address: SolAddress,
	opened_at: SlotNumber,
	expires_at: SlotNumber,
	epochs: Ep,
	sol_client: Arc<SolanaClient>,
	kill_switch: Arc<AtomicBool>,
}

fn single_deposit_address_stream<EpochStream, SolanaClient, EpochI, EpochHI>(
	TrackerArgs {
		channel_id,
		address,
		opened_at,
		expires_at,
		epochs: epoch_stream,
		sol_client,
		kill_switch,
	}: TrackerArgs<SolanaClient, EpochStream>,
) -> impl Stream<Item = Result<(ChannelId, SolAddress, Epoch<EpochI, EpochHI>, Balance)>> + Send
where
	EpochStream: Stream<Item = Epoch<EpochI, EpochHI>> + Send + Sized,
	SolanaClient: SolanaApi + Send + Sync + 'static,
	Epoch<EpochI, EpochHI>: Send + Sync + Clone + 'static,
{
	tracing::info!(
		"starting up a deposit-address tracking for #{}: {} [{}..{}]",
		channel_id,
		address,
		opened_at,
		expires_at,
	);
	AddressSignatures::new(Arc::clone(&sol_client), address, kill_switch)
		.max_page_size(SOLANA_SIGNATURES_FOR_TRANSACTION_PAGE_SIZE)
		.poll_interval(SOLANA_SIGNATURES_FOR_TRANSACTION_POLL_INTERVAL)
		// // TODO: find a way to start from where we may have left
		// .after_transaction(last_known_transaction)
		.starting_with_slot(opened_at)
		.ending_with_slot(expires_at)
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
		.try_zip_latest(epoch_stream)
		.map_ok(move |(balance, epoch)| (channel_id, address, epoch, balance))
		.inspect_ok(move |(channel_id, address, epoch, balance)| {
			tracing::debug!(
				"deposit-address tracker #{} [addr: {}, lifetime: {}, balance: {:?}]",
				channel_id,
				address,
				epoch.index,
				balance
			)
		})
		.inspect_err(move |reason| {
			tracing::warn!(
				"Failure on deposit-address tracker #{} [addr: {}; lifetime: {}..{}]: {}",
				channel_id,
				address,
				opened_at,
				expires_at,
				reason
			)
		})
}
