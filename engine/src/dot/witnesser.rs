use std::{collections::HashMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use cf_chains::dot::{
	self, Polkadot, PolkadotAccountId, PolkadotBalance, PolkadotExtrinsicIndex, PolkadotHash,
	PolkadotProxyType, PolkadotSignature, PolkadotUncheckedExtrinsic,
};
use cf_primitives::{chains::assets, EpochIndex, PolkadotBlockNumber, TxId};
use codec::{Decode, Encode};
use frame_support::scale_info::TypeInfo;
use futures::{stream, Stream, StreamExt, TryStreamExt};
use pallet_cf_ingress_egress::DepositWitness;
use sp_core::H256;
use state_chain_runtime::PolkadotInstance;
use subxt::{
	config::Header,
	events::{Phase, StaticEvent},
};
use tokio::sync::Mutex;
use tracing::{debug, error, info, info_span, trace, Instrument};

use crate::{
	constants::{BLOCK_PULL_TIMEOUT_MULTIPLIER, DOT_AVERAGE_BLOCK_TIME_SECONDS},
	db::PersistentKeyDB,
	state_chain_observer::client::extrinsic_api::signed::SignedExtrinsicApi,
	witnesser::{
		block_head_stream_from::block_head_stream_from,
		block_witnesser::{
			BlockStream, BlockWitnesser, BlockWitnesserGenerator, BlockWitnesserGeneratorWrapper,
		},
		epoch_process_runner::{start_epoch_process_runner, EpochProcessRunnerError},
		ChainBlockNumber, EpochStart, HasBlockNumber, ItemMonitor,
	},
};

use anyhow::{Context, Result};

use super::rpc::{DotRpcApi, DotSubscribeApi};

#[derive(Debug, Clone, Copy)]
pub struct MiniHeader {
	block_number: PolkadotBlockNumber,
	block_hash: PolkadotHash,
}

impl HasBlockNumber for MiniHeader {
	type BlockNumber = PolkadotBlockNumber;

	fn block_number(&self) -> Self::BlockNumber {
		self.block_number
	}
}

/// This event represents a rotation of the agg key. We have handed over control of the vault
/// to the new aggregrate at this event.
#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
pub struct ProxyAdded {
	delegator: PolkadotAccountId,
	delegatee: PolkadotAccountId,
	proxy_type: PolkadotProxyType,
	delay: PolkadotBlockNumber,
}

impl StaticEvent for ProxyAdded {
	const PALLET: &'static str = "Proxy";
	const EVENT: &'static str = "ProxyAdded";
}

/// This event must match the Transfer event definition of the Polkadot chain.
#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
pub struct Transfer {
	from: PolkadotAccountId,
	to: PolkadotAccountId,
	amount: PolkadotBalance,
}

impl StaticEvent for Transfer {
	const PALLET: &'static str = "Balances";
	const EVENT: &'static str = "Transfer";
}

/// This event must match the TransactionFeePaid event definition of the Polkadot chain.
#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
struct TransactionFeePaid {
	who: PolkadotAccountId,
	// includes the tip
	actual_fee: PolkadotBalance,
	tip: PolkadotBalance,
}

impl StaticEvent for TransactionFeePaid {
	const PALLET: &'static str = "TransactionPayment";
	const EVENT: &'static str = "TransactionFeePaid";
}

#[derive(Clone)]
enum EventWrapper {
	ProxyAdded(ProxyAdded),
	Transfer(Transfer),
	TransactionFeePaid(TransactionFeePaid),
}

/// Takes a stream of Results and terminates when it hits an error, logging the error before
/// terminating.
fn take_while_ok<InStream, T, E>(inner_stream: InStream) -> impl Stream<Item = T>
where
	InStream: Stream<Item = std::result::Result<T, E>> + Send,
	E: std::fmt::Debug,
{
	struct StreamState<FromStream, T, E>
	where
		FromStream: Stream<Item = std::result::Result<T, E>>,
	{
		stream: FromStream,
	}

	let init_state = StreamState { stream: Box::pin(inner_stream) };

	stream::unfold(init_state, move |mut state| async move {
		match state.stream.next().await {
			Some(Ok(item)) => Some((item, state)),
			Some(Err(e)) => {
				error!("Error on stream: {e:?}");
				None
			},
			None => None,
		}
	})
}

#[allow(clippy::vec_box)]
fn check_for_interesting_events_in_block(
	block_events: Vec<(Phase, EventWrapper)>,
	block_number: PolkadotBlockNumber,
	our_vault: &PolkadotAccountId,
	address_monitor: &mut Arc<Mutex<ItemMonitor<PolkadotAccountId, PolkadotAccountId, ()>>>,
) -> (
	Vec<(PolkadotExtrinsicIndex, PolkadotBalance)>,
	Vec<DepositWitness<Polkadot>>,
	Vec<Box<state_chain_runtime::RuntimeCall>>,
	// Median tip of all extrinsics in this block
	u128,
) {
	// to contain all the deposit witnesses for this block
	let mut deposit_witnesses = Vec::new();
	// We only want to attempt to decode extrinsics that were sent by us. Since
	// a) we know how to decode calls we create (but not necessarily all calls on Polkadot)
	// b) these are the only extrinsics we are interested in
	let mut interesting_indices = Vec::new();
	let mut vault_key_rotated_calls: Vec<Box<_>> = Vec::new();
	let mut fee_paid_for_xt_at_index = HashMap::new();
	let events_iter = block_events.iter();
	let mut tips = Vec::<PolkadotBalance>::new();

	let mut address_monitor = address_monitor.try_lock().expect("should have exclusive ownership");

	for (phase, wrapped_event) in events_iter {
		if let Phase::ApplyExtrinsic(extrinsic_index) = *phase {
			match wrapped_event {
				EventWrapper::ProxyAdded(ProxyAdded { delegator, delegatee, .. }) => {
					if delegator != our_vault {
						continue
					}

					interesting_indices.push(extrinsic_index);

					info!("Witnessing ProxyAdded. new delegatee: {delegatee:?} at block number {block_number} and extrinsic_index; {extrinsic_index}");

					vault_key_rotated_calls.push(Box::new(
						pallet_cf_vaults::Call::<_, PolkadotInstance>::vault_key_rotated {
							block_number,
							tx_id: TxId { block_number, extrinsic_index },
						}
						.into(),
					));
				},
				EventWrapper::Transfer(Transfer { to, amount, from }) => {
					// When we get a transfer event, we want to check that we have
					// pulled the latest addresses to monitor from the chain first
					address_monitor.sync_items();

					if address_monitor.contains(to) {
						info!("Witnessing DOT Ingress {{ amount: {amount:?}, to: {to:?} }}");
						deposit_witnesses.push(DepositWitness {
							deposit_address: *to,
							asset: assets::dot::Asset::Dot,
							amount: *amount,
							tx_id: TxId { block_number, extrinsic_index },
						});
					}

					// if `from` is our_vault then we're doing an egress
					// if `to` is our_vault then we're doing a "deposit fetch"
					if from == our_vault || to == our_vault {
						info!("Transfer from or to our_vault at block: {block_number}, extrinsic index: {extrinsic_index}");
						interesting_indices.push(extrinsic_index);
					}
				},
				EventWrapper::TransactionFeePaid(TransactionFeePaid {
					actual_fee, tip, ..
				}) => {
					fee_paid_for_xt_at_index.insert(extrinsic_index, *actual_fee);
					tips.push(*tip);
				},
			}
		}
	}

	tips.sort();
	let median_tip = tips
		.get({
			let len = tips.len();
			if len % 2 == 0 {
				(len / 2).saturating_sub(1)
			} else {
				len / 2
			}
		})
		.cloned()
		.unwrap_or_default();

	(
		interesting_indices
			.into_iter()
			.map(|index| (index, *fee_paid_for_xt_at_index.get(&index).unwrap()))
			.collect(),
		deposit_witnesses,
		vault_key_rotated_calls,
		median_tip,
	)
}

/// Polkadot witnesser
///
/// This component does all witnessing activities for Polkadot. This includes rotation witnessing,
/// deposit witnessing and broadcast/egress witnessing.
///
/// We use events for rotation and deposit witnessing but for broadcast/egress witnessing we use the
/// signature of the extrinsic.
pub async fn start<StateChainClient, DotRpc>(
	resume_at: Option<EpochStart<Polkadot>>,
	epoch_starts_receiver: async_broadcast::Receiver<EpochStart<Polkadot>>,
	dot_client: DotRpc,
	address_monitor: Arc<Mutex<ItemMonitor<PolkadotAccountId, PolkadotAccountId, ()>>>,
	signature_monitor: Arc<Mutex<ItemMonitor<PolkadotSignature, PolkadotSignature, ()>>>,
	state_chain_client: Arc<StateChainClient>,
	db: Arc<PersistentKeyDB>,
) -> std::result::Result<(), EpochProcessRunnerError<Polkadot>>
where
	StateChainClient: SignedExtrinsicApi + 'static + Send + Sync,
	DotRpc: DotSubscribeApi + DotRpcApi + 'static + Send + Sync + Clone,
{
	start_epoch_process_runner(
		resume_at,
		Arc::new(Mutex::new(epoch_starts_receiver)),
		BlockWitnesserGeneratorWrapper {
			generator: DotWitnesserGenerator { state_chain_client, dot_client },
			db,
		},
		(address_monitor, signature_monitor),
	)
	.instrument(info_span!("Dot-Witnesser"))
	.await
}

// An instance of a Polkadot Witnesser for a particular epoch.
struct DotBlockWitnesser<StateChainClient, DotRpc> {
	state_chain_client: Arc<StateChainClient>,
	dot_client: DotRpc,
	epoch_index: EpochIndex,
	// The account id of our Polkadot vault.
	vault_account: cf_chains::dot::PolkadotAccountId,
}

impl HasBlockNumber for (H256, PolkadotBlockNumber, Vec<(Phase, EventWrapper)>) {
	type BlockNumber = PolkadotBlockNumber;

	fn block_number(&self) -> Self::BlockNumber {
		self.1
	}
}

#[async_trait]
impl<StateChainClient, DotRpc> BlockWitnesser for DotBlockWitnesser<StateChainClient, DotRpc>
where
	StateChainClient: SignedExtrinsicApi + 'static + Send + Sync,
	DotRpc: DotRpcApi + 'static + Send + Sync + Clone,
{
	type Chain = Polkadot;
	type Block = (H256, u32, Vec<(Phase, EventWrapper)>);
	type StaticState = (
		Arc<Mutex<ItemMonitor<PolkadotAccountId, PolkadotAccountId, ()>>>,
		Arc<Mutex<ItemMonitor<PolkadotSignature, PolkadotSignature, ()>>>,
	);

	async fn process_block(
		&mut self,
		data: Self::Block,
		(address_monitor, signature_monitor): &mut Self::StaticState,
	) -> anyhow::Result<()> {
		let (block_hash, block_number, block_event_details) = data;
		trace!("Checking block: {block_number}, with hash: {block_hash:?} for interesting events");

		let mut signature_monitor =
			signature_monitor.try_lock().expect("should have exclusive ownership");

		let (interesting_indices, deposit_witnesses, vault_key_rotated_calls, median_tip) =
			check_for_interesting_events_in_block(
				block_event_details,
				block_number,
				&self.vault_account,
				address_monitor,
			);

		self.state_chain_client
			.submit_signed_extrinsic(state_chain_runtime::RuntimeCall::Witnesser(
				pallet_cf_witnesser::Call::witness_at_epoch {
					call: Box::new(state_chain_runtime::RuntimeCall::PolkadotChainTracking(
						pallet_cf_chain_tracking::Call::update_chain_state {
							state: dot::PolkadotTrackedData {
								block_height: block_number,
								median_tip,
							},
						},
					)),
					epoch_index: self.epoch_index,
				},
			))
			.await;

		for call in vault_key_rotated_calls {
			self.state_chain_client
				.submit_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
					call,
					epoch_index: self.epoch_index,
				})
				.await;
		}

		if !interesting_indices.is_empty() {
			info!("We got an interesting block at block: {block_number}, hash: {block_hash:?}");

			let extrinsics = self
				.dot_client
				.extrinsics(block_hash)
				.await
				.context("Failed fetching block from DOT RPC")?
				.context(
					format!("Polkadot block does not exist for block hash: {block_hash:?}",),
				)?;

			signature_monitor.sync_items();
			for (extrinsic_index, tx_fee) in interesting_indices {
				let xt = extrinsics.get(extrinsic_index as usize).expect("We know this exists since we got this index from the event, from the block we are querying.");
				let xt_encoded = xt.0.encode();
				let mut xt_bytes = xt_encoded.as_slice();
				let unchecked = PolkadotUncheckedExtrinsic::decode(&mut xt_bytes);
				if let Ok(unchecked) = unchecked {
					if let Some(signature) = unchecked.signature() {
						if signature_monitor.remove(&signature) {
							info!("Witnessing transaction_succeeded. signature: {signature:?}");

							self.state_chain_client
								.submit_signed_extrinsic(
									pallet_cf_witnesser::Call::witness_at_epoch {
										call:
											Box::new(
												pallet_cf_broadcast::Call::<
													_,
													PolkadotInstance,
												>::transaction_succeeded {
													tx_out_id: signature,
													block_number,
													signer_id: self.vault_account,
													tx_fee,
												}
												.into(),
											),
										epoch_index: self.epoch_index,
									},
								)
								.await;
						}
					}
				} else {
					// We expect this to occur when attempting to decode
					// a transaction that was not sent by us.
					// We can safely ignore it, but we log it in case.
					debug!("Failed to decode UncheckedExtrinsic {unchecked:?}");
				}
			}
		}

		if !deposit_witnesses.is_empty() {
			self.state_chain_client
				.submit_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
					call: Box::new(
						pallet_cf_ingress_egress::Call::<_, PolkadotInstance>::process_deposits {
							deposit_witnesses,
						}
						.into(),
					),
					epoch_index: self.epoch_index,
				})
				.await;
		}

		Ok(())
	}
}

struct DotWitnesserGenerator<StateChainClient, DotRpc> {
	state_chain_client: Arc<StateChainClient>,
	dot_client: DotRpc,
}

#[async_trait]
impl<StateChainClient, DotRpc> BlockWitnesserGenerator
	for DotWitnesserGenerator<StateChainClient, DotRpc>
where
	StateChainClient: SignedExtrinsicApi + 'static + Send + Sync,
	DotRpc: DotRpcApi + DotSubscribeApi + 'static + Send + Sync + Clone,
{
	type Witnesser = DotBlockWitnesser<StateChainClient, DotRpc>;

	fn create_witnesser(
		&self,
		epoch: EpochStart<<Self::Witnesser as BlockWitnesser>::Chain>,
	) -> Self::Witnesser {
		DotBlockWitnesser {
			state_chain_client: self.state_chain_client.clone(),
			dot_client: self.dot_client.clone(),
			epoch_index: epoch.epoch_index,
			vault_account: epoch.data.vault_account,
		}
	}

	async fn get_block_stream(
		&mut self,
		from_block: ChainBlockNumber<<Self::Witnesser as BlockWitnesser>::Chain>,
	) -> anyhow::Result<BlockStream<<Self::Witnesser as BlockWitnesser>::Block>> {
		let safe_head_stream = take_while_ok(self.dot_client.subscribe_finalized_heads().await?)
			.map(|header| MiniHeader { block_number: header.number, block_hash: header.hash() });

		let dot_client_c = self.dot_client.clone();
		let block_head_stream_from =
			block_head_stream_from(from_block, safe_head_stream, move |block_number| {
				let mut dot_client = dot_client_c.clone();
				Box::pin(async move {
					let block_hash = dot_client
						.block_hash(block_number)
						.await?
						.expect("Called on a finalised stream, so the block will exist");
					Ok(MiniHeader { block_number, block_hash })
				})
			})
			.await?;

		// Stream of Events objects. Each `Events` contains the events for a particular
		// block
		let dot_client_c = self.dot_client.clone();
		let block_events_stream = take_while_ok(block_head_stream_from.then(move |mini_header| {
			let mut dot_client = dot_client_c.clone();
			debug!("Fetching Polkadot events for block: {}", mini_header.block_number);
			// TODO: This will not work if the block we are querying metadata has
			// different metadata than the latest block since this client fetches
			// the latest metadata and always uses it.
			// https://github.com/chainflip-io/chainflip-backend/issues/2542
			async move {
				Result::<_, anyhow::Error>::Ok((
					mini_header.block_hash,
					mini_header.block_number,
					dot_client.events(mini_header.block_hash).await?,
				))
			}
		}))
		.map(|(block_hash, block_number, events)| {
			(
				block_hash,
				block_number,
				events
					.iter()
					.filter_map(|event_details| match event_details {
						Ok(event_details) =>
							match (event_details.pallet_name(), event_details.variant_name()) {
								(ProxyAdded::PALLET, ProxyAdded::EVENT) =>
									Some(EventWrapper::ProxyAdded(
										event_details.as_event::<ProxyAdded>().unwrap().unwrap(),
									)),
								(Transfer::PALLET, Transfer::EVENT) =>
									Some(EventWrapper::Transfer(
										event_details.as_event::<Transfer>().unwrap().unwrap(),
									)),
								(TransactionFeePaid::PALLET, TransactionFeePaid::EVENT) =>
									Some(EventWrapper::TransactionFeePaid(
										event_details
											.as_event::<TransactionFeePaid>()
											.unwrap()
											.unwrap(),
									)),
								_ => None,
							}
							.map(|event| (event_details.phase(), event)),
						Err(err) => {
							error!("Error while parsing event: {:?}", err);
							None
						},
					})
					.collect(),
			)
		});

		let block_events_stream = tokio_stream::StreamExt::timeout(
			block_events_stream,
			Duration::from_secs(DOT_AVERAGE_BLOCK_TIME_SECONDS * BLOCK_PULL_TIMEOUT_MULTIPLIER),
		)
		.map_err(|err| {
			error!("Error while fetching Polkadot events: {:?}", err);
			anyhow::anyhow!("Error while fetching Polkadot events: {:?}", err)
		})
		.chain(stream::once(async {
			error!("Stream ended unexpectedly");
			Err(anyhow::anyhow!("Stream ended unexpectedly"))
		}));

		Ok(Box::pin(block_events_stream))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	use cf_chains::dot::{self, PolkadotAccountId};
	use itertools::Itertools;
	use std::collections::BTreeSet;

	use crate::{
		dot::rpc::DotRpcClient, state_chain_observer::client::mocks::MockStateChainClient,
		witnesser::MonitorCommand,
	};

	fn mock_proxy_added(
		delegator: &PolkadotAccountId,
		delegatee: &PolkadotAccountId,
	) -> EventWrapper {
		EventWrapper::ProxyAdded(ProxyAdded {
			delegator: *delegator,
			delegatee: *delegatee,
			proxy_type: PolkadotProxyType::Any,
			delay: 0,
		})
	}

	fn mock_tx_fee_paid(actual_fee: PolkadotBalance) -> EventWrapper {
		EventWrapper::TransactionFeePaid(TransactionFeePaid {
			actual_fee,
			who: PolkadotAccountId::from_aliased([0xab; 32]),
			tip: Default::default(),
		})
	}

	fn mock_tx_fee_paid_tip(tip: PolkadotBalance) -> EventWrapper {
		EventWrapper::TransactionFeePaid(TransactionFeePaid {
			actual_fee: Default::default(),
			who: PolkadotAccountId::from_aliased([0xab; 32]),
			tip,
		})
	}

	fn mock_transfer(
		from: &PolkadotAccountId,
		to: &PolkadotAccountId,
		amount: PolkadotBalance,
	) -> EventWrapper {
		EventWrapper::Transfer(Transfer { from: *from, to: *to, amount })
	}

	fn phase_and_events(
		events: &[(PolkadotExtrinsicIndex, EventWrapper)],
	) -> Vec<(Phase, EventWrapper)> {
		events
			.iter()
			.map(|(xt_index, event)| (Phase::ApplyExtrinsic(*xt_index), event.clone()))
			.collect()
	}

	#[test]
	fn proxy_added_event_for_our_vault_witnessed() {
		let our_vault = PolkadotAccountId::from_aliased([0; 32]);
		let other_acct = PolkadotAccountId::from_aliased([1; 32]);
		let our_proxy_added_index = 1u32;
		let fee_paid = 10000;
		let block_event_details = phase_and_events(&[
			// we should witness this one
			(our_proxy_added_index, mock_proxy_added(&our_vault, &other_acct)),
			(our_proxy_added_index, mock_tx_fee_paid(fee_paid)),
			// we should not witness this one
			(3u32, mock_proxy_added(&other_acct, &our_vault)),
			(3u32, mock_tx_fee_paid(20000)),
		]);

		let (mut interesting_indices, deposit_witnesses, vault_key_rotated_calls, _) =
			check_for_interesting_events_in_block(
				block_event_details,
				Default::default(),
				&our_vault,
				&mut Arc::new(Mutex::new(ItemMonitor::new(Default::default()).1)),
			);

		assert_eq!(vault_key_rotated_calls.len(), 1);
		assert_eq!(interesting_indices.pop().unwrap(), (our_proxy_added_index, fee_paid));
		assert!(deposit_witnesses.is_empty());
	}

	#[test]
	fn witness_deposits_for_addresses_we_monitor() {
		// we want two monitors, one sent through at start, and one sent through channel
		const TRANSFER_1_INDEX: u32 = 1;
		let transfer_1_deposit_address = PolkadotAccountId::from_aliased([1; 32]);
		const TRANSFER_1_AMOUNT: PolkadotBalance = 10000;

		const TRANSFER_2_INDEX: u32 = 2;
		let transfer_2_deposit_address = PolkadotAccountId::from_aliased([2; 32]);
		const TRANSFER_2_AMOUNT: PolkadotBalance = 20000;

		let block_event_details = phase_and_events(&[
			// we'll be witnessing this from the start
			(
				TRANSFER_1_INDEX,
				mock_transfer(
					&PolkadotAccountId::from_aliased([7; 32]),
					&transfer_1_deposit_address,
					TRANSFER_1_AMOUNT,
				),
			),
			// we'll receive this address from the channel
			(
				TRANSFER_2_INDEX,
				mock_transfer(
					&PolkadotAccountId::from_aliased([7; 32]),
					&transfer_2_deposit_address,
					TRANSFER_2_AMOUNT,
				),
			),
			// this one is not for us
			(
				19,
				mock_transfer(
					&PolkadotAccountId::from_aliased([7; 32]),
					&PolkadotAccountId::from_aliased([9; 32]),
					93232,
				),
			),
		]);

		let (monitor_command_sender, address_monitor) =
			ItemMonitor::new(BTreeSet::from([transfer_1_deposit_address]));

		monitor_command_sender
			.send(MonitorCommand::Add(transfer_2_deposit_address))
			.unwrap();

		let (interesting_indices, deposit_witnesses, vault_key_rotated_calls, _) =
			check_for_interesting_events_in_block(
				block_event_details,
				20,
				// arbitrary, not focus of the test
				&PolkadotAccountId::from_aliased([0xda; 32]),
				&mut Arc::new(Mutex::new(address_monitor)),
			);

		assert_eq!(deposit_witnesses.len(), 2);
		assert_eq!(deposit_witnesses.get(0).unwrap().amount, TRANSFER_1_AMOUNT);
		assert_eq!(deposit_witnesses.get(1).unwrap().amount, TRANSFER_2_AMOUNT);

		// We don't need to submit signature accepted for deposit witnesses
		assert_eq!(interesting_indices.len(), 0);
		assert!(vault_key_rotated_calls.is_empty());
	}

	#[test]
	fn deposit_fetch_and_egress_witnessed() {
		let egress_index = 3;
		let egress_amount = 30000;

		let deposit_fetch_index = 4;
		let deposit_fetch_amount = 40000;
		let our_vault = PolkadotAccountId::from_aliased([3; 32]);

		let block_event_details = phase_and_events(&[
			// we'll be witnessing this from the start
			(
				egress_index,
				// egress, from our vault
				mock_transfer(&our_vault, &PolkadotAccountId::from_aliased([6; 32]), egress_amount),
			),
			// fee same as amount for simpler testing
			(egress_index, mock_tx_fee_paid(egress_amount)),
			// we'll receive this address from the channel
			(
				deposit_fetch_index,
				// fetch deposit, to our vault
				mock_transfer(
					&PolkadotAccountId::from_aliased([7; 32]),
					&our_vault,
					deposit_fetch_amount,
				),
			),
			(deposit_fetch_index, mock_tx_fee_paid(deposit_fetch_amount)),
			// this one is not for us
			(
				19,
				mock_transfer(
					&PolkadotAccountId::from_aliased([7; 32]),
					&PolkadotAccountId::from_aliased([9; 32]),
					93232,
				),
			),
		]);

		let (interesting_indices, deposit_witnesses, vault_key_rotated_calls, _) =
			check_for_interesting_events_in_block(
				block_event_details,
				20,
				// arbitrary, not focus of the test
				&our_vault,
				&mut Arc::new(Mutex::new(ItemMonitor::new(BTreeSet::default()).1)),
			);

		assert!(
			interesting_indices.contains(&(egress_index, egress_amount)) &&
				interesting_indices.contains(&(deposit_fetch_index, deposit_fetch_amount))
		);

		assert!(vault_key_rotated_calls.is_empty());
		assert!(deposit_witnesses.is_empty());
	}

	#[test]
	fn test_median_tip_calculation() {
		test_median_tip_calculated_from_events_correctly(&[], 0);
		test_median_tip_calculated_from_events_correctly(&[10], 10);
		test_median_tip_calculated_from_events_correctly(&[10, 100], 10);
		test_median_tip_calculated_from_events_correctly(&[10, 100, 1000], 100);
		test_median_tip_calculated_from_events_correctly(&[10, 100, 1000, 1000], 100);
	}

	fn test_median_tip_calculated_from_events_correctly(
		test_case: &[PolkadotBalance],
		expected_median: PolkadotBalance,
	) {
		let num_permutations = if test_case.is_empty() { 1 } else { test_case.len() };
		for tips in test_case.iter().permutations(num_permutations) {
			let block_event_details = phase_and_events(
				(1..)
					.zip(&tips)
					.map(|(i, &&tip)| (i, mock_tx_fee_paid_tip(tip)))
					.collect::<Vec<_>>()
					.as_slice(),
			);

			let (.., median_tip) = check_for_interesting_events_in_block(
				block_event_details,
				20,
				// arbitrary, not focus of the test
				&PolkadotAccountId::from_aliased([0xda; 32]),
				&mut Arc::new(Mutex::new(ItemMonitor::new(BTreeSet::default()).1)),
			);

			assert_eq!(
				median_tip, expected_median,
				"Incorrect median value for input {tips:?}. Expected {expected_median:?}",
			);
		}
	}

	#[ignore = "This test is helpful for local testing. Requires connection to westend"]
	#[tokio::test]
	async fn start_witnessing() {
		let url = "ws://localhost:9944";

		println!("Connecting to: {url}");
		let dot_rpc_client = DotRpcClient::new(url).await.unwrap();

		let (epoch_starts_sender, epoch_starts_receiver) = async_broadcast::broadcast(10);

		let state_chain_client = Arc::new(MockStateChainClient::new());

		// proxy type any
		// epoch_starts_sender
		// 	.broadcast(EpochStart {
		// 		epoch_index: 3,
		// 		block_number: 13544356,
		// 		current: true,
		// 		participant: true,
		// 		data: dot::EpochStartData {
		// 			vault_account: PolkadotAccountId::from_str(
		// 				"5EsWs6A7fT2X7AP4hwQUMzi4Aixz6hbtUZB3EAdpfRS4Qv36",
		// 			)
		// 			.unwrap(),
		// 		},
		// 	})
		// 	.await
		// 	.unwrap();

		// Monitor for transfers
		// dot_monitor_command_sender
		// 	.send(
		// 		PolkadotAccountId::from_str("5DJVVEYPDFZjj9JtJRE2vGvpeSnzBAUA74VXPSpkGKhJSHbN")
		// 			.unwrap(),
		// 	)
		// 	.unwrap();

		let polkadot_sig = PolkadotSignature::from_aliased(hex::decode("7c388203aefbcdc22077ed91bec9af80a23c56f8ff2ee24d40f4c2791d51773342f4aed0e8f0652ed33d404d9b78366a927be9fad02f5204f2f2ffbea7459886").unwrap().try_into().unwrap());

		let (signature_sender, signature_monitor) = ItemMonitor::new(BTreeSet::default());

		signature_sender.send(MonitorCommand::Add(polkadot_sig)).unwrap();

		// proxy type governance
		epoch_starts_sender
			.broadcast(EpochStart {
				epoch_index: 1,
				block_number: 0,
				current: true,
				participant: true,
				data: dot::EpochStartData {
					vault_account: PolkadotAccountId::from_ss58check(
						"12RtzLB2z2dsg9RUGtTuxhuyzsTFScYG8hBT7hJcaNbFqCry",
					)
					.unwrap(),
				},
			})
			.await
			.unwrap();

		let (_dir, db_path) = utilities::testing::new_temp_directory_with_nonexistent_file();
		let db = PersistentKeyDB::open_and_migrate_to_latest(&db_path, None).unwrap();

		start(
			None,
			epoch_starts_receiver,
			dot_rpc_client,
			Arc::new(Mutex::new(ItemMonitor::new(Default::default()).1)),
			Arc::new(Mutex::new(signature_monitor)),
			state_chain_client,
			Arc::new(db),
		)
		.await
		.unwrap();
	}
}
