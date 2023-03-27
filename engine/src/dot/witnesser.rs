use std::{
	collections::{BTreeSet, HashMap},
	sync::Arc,
};

use cf_chains::{
	dot::{
		Polkadot, PolkadotBalance, PolkadotExtrinsicIndex, PolkadotHash, PolkadotProxyType,
		PolkadotPublicKey, PolkadotUncheckedExtrinsic,
	},
	eth::assets,
};
use cf_primitives::{PolkadotAccountId, PolkadotBlockNumber, TxId};
use codec::{Decode, Encode};
use frame_support::scale_info::TypeInfo;
use futures::{stream, Stream, StreamExt};
use pallet_cf_ingress_egress::IngressWitness;
use sp_runtime::MultiSignature;
use state_chain_runtime::PolkadotInstance;
use subxt::{
	config::Header,
	events::{Phase, StaticEvent},
};
use tokio::{select, sync::Mutex};
use tracing::{debug, error, info, info_span, trace, Instrument};

use crate::{
	multisig::{ChainTag, PersistentKeyDB},
	state_chain_observer::client::extrinsic_api::ExtrinsicApi,
	witnesser::{
		block_head_stream_from::block_head_stream_from,
		checkpointing::{
			get_witnesser_start_block_with_checkpointing, StartCheckpointing, WitnessedUntil,
		},
		epoch_witnesser::{self},
		AddressMonitor, BlockNumberable, EpochStart,
	},
};

use anyhow::{Context, Result};

use super::rpc::DotRpcApi;

#[derive(Debug, Clone, Copy)]
pub struct MiniHeader {
	pub block_number: PolkadotBlockNumber,
	block_hash: PolkadotHash,
}

impl BlockNumberable for MiniHeader {
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
	address_monitor: &mut AddressMonitor<PolkadotAccountId, PolkadotAccountId, ()>,
) -> (
	Vec<(PolkadotExtrinsicIndex, PolkadotBalance)>,
	Vec<IngressWitness<Polkadot>>,
	Vec<Box<state_chain_runtime::RuntimeCall>>,
) {
	// to contain all the ingress witnessse for this block
	let mut ingress_witnesses = Vec::new();
	// We only want to attempt to decode extrinsics that were sent by us. Since
	// a) we know how to decode calls we create (but not necessarily all calls on Polkadot)
	// b) these are the only extrinsics we are interested in
	let mut interesting_indices = Vec::new();
	let mut vault_key_rotated_calls: Vec<Box<_>> = Vec::new();
	let mut fee_paid_for_xt_at_index = HashMap::new();
	let events_iter = block_events.iter();
	for (phase, wrapped_event) in events_iter {
		if let Phase::ApplyExtrinsic(extrinsic_index) = *phase {
			match wrapped_event {
				EventWrapper::ProxyAdded(ProxyAdded { delegator, delegatee, .. }) => {
					if AsRef::<[u8; 32]>::as_ref(&delegator) !=
						AsRef::<[u8; 32]>::as_ref(&our_vault)
					{
						continue
					}

					interesting_indices.push(extrinsic_index);

					let new_public_key =
						PolkadotPublicKey::from(*AsRef::<[u8; 32]>::as_ref(&delegatee));
					info!("Witnessing ProxyAdded. new public key: {new_public_key:?} at block number {block_number} and extrinsic_index; {extrinsic_index}");

					vault_key_rotated_calls.push(Box::new(
						pallet_cf_vaults::Call::<_, PolkadotInstance>::vault_key_rotated {
							new_public_key,
							block_number,
							tx_id: TxId { block_number, extrinsic_index },
						}
						.into(),
					));
				},
				EventWrapper::Transfer(Transfer { to, amount, from }) => {
					// When we get a transfer event, we want to check that we have
					// pulled the latest addresses to monitor from the chain first
					address_monitor.sync_addresses();

					if address_monitor.contains(to) {
						info!("Witnessing DOT Ingress {{ amount: {amount:?}, to: {to:?} }}");
						ingress_witnesses.push(IngressWitness {
							ingress_address: to.clone(),
							asset: assets::dot::Asset::Dot,
							amount: *amount,
							tx_id: TxId { block_number, extrinsic_index },
						});
					}

					// if `from` is our_vault then we're doing an egress
					// if `to` is our_vault then we're doing an "ingress fetch"
					if from == our_vault || to == our_vault {
						info!("Transfer from or to our_vault at block: {block_number}, extrinsic index: {extrinsic_index}");
						interesting_indices.push(extrinsic_index);
					}
				},
				EventWrapper::TransactionFeePaid(TransactionFeePaid { actual_fee, .. }) => {
					fee_paid_for_xt_at_index.insert(extrinsic_index, *actual_fee);
				},
			}
		}
	}

	(
		interesting_indices
			.into_iter()
			.map(|index| (index, *fee_paid_for_xt_at_index.get(&index).unwrap()))
			.collect(),
		ingress_witnesses,
		vault_key_rotated_calls,
	)
}

/// Polkadot witnesser
///
/// This component does all witnessing activities for Polkadot. This includes rotation witnessing,
/// ingress witnessing and broadcast/egress witnessing.
///
/// We use events for rotation and ingress witnessing but for broadcast/egress witnessing we use the
/// signature of the extrinsic.
pub async fn start<StateChainClient, DotRpc>(
	epoch_starts_receiver: async_broadcast::Receiver<EpochStart<Polkadot>>,
	dot_client: DotRpc,
	address_monitor: AddressMonitor<PolkadotAccountId, PolkadotAccountId, ()>,
	signature_receiver: tokio::sync::mpsc::UnboundedReceiver<[u8; 64]>,
	monitored_signatures: BTreeSet<[u8; 64]>,
	state_chain_client: Arc<StateChainClient>,
	db: Arc<PersistentKeyDB>,
) -> std::result::Result<(), anyhow::Error>
where
	StateChainClient: ExtrinsicApi + 'static + Send + Sync,
	DotRpc: DotRpcApi + 'static + Send + Sync + Clone,
{
	epoch_witnesser::start(
		Arc::new(Mutex::new(epoch_starts_receiver)),
		|_epoch_start| true,
		(address_monitor, monitored_signatures, signature_receiver),
		move |
		mut end_witnessing_receiver,
			epoch_start,
			(
				mut address_monitor,
				mut monitored_signatures,
				mut signature_receiver
			),
		| {
			let mut dot_client = dot_client.clone();
			let state_chain_client = state_chain_client.clone();
			let db = db.clone();
			async move {
				let (from_block, witnessed_until_sender) = match get_witnesser_start_block_with_checkpointing::<Polkadot>(
					ChainTag::Polkadot,
					epoch_start.epoch_index,
					epoch_start.block_number,
					db,
				).await
				.expect("Failed to start Dot witnesser checkpointing")
				{
					StartCheckpointing::Started((from_block, witnessed_until_sender)) =>
						(from_block, witnessed_until_sender),
					StartCheckpointing::AlreadyWitnessedEpoch =>
						return Ok((
							address_monitor,
							monitored_signatures,
							signature_receiver
						)),
				};

				let safe_head_stream =
					take_while_ok(dot_client.subscribe_finalized_heads().await?)
						.map(|header| MiniHeader {
							block_number: header.number,
							block_hash: header.hash(),
						});

				let dot_client_c = dot_client.clone();
				let block_head_stream_from = block_head_stream_from(from_block, safe_head_stream, move |block_number| {
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

				let our_vault = epoch_start.data.vault_account;

				// Stream of Events objects. Each `Events` contains the events for a particular
				// block
				let dot_client_c = dot_client.clone();
				let mut block_events_stream = Box::pin(
					take_while_ok(
						block_head_stream_from.then(|mini_header| {
							let mut dot_client = dot_client_c.clone();
							debug!(
								"Fetching Polkadot events for block: {}",
								mini_header.block_number
							);
							// TODO: This will not work if the block we are querying metadata has
							// different metadata than the latest block since this client fetches
							// the latest metadata and always uses it.
							// https://github.com/chainflip-io/chainflip-backend/issues/2542
							async move {
								Result::<_, anyhow::Error>::Ok((
									mini_header.block_hash,
									mini_header.block_number,
									dot_client.events(mini_header.block_hash).await?
								))
							}
						}),
					)
					.map(|(block_hash, block_number, events)| {
						(block_hash, block_number, events.iter().filter_map(|event_details| {
							match event_details {
								Ok(event_details) => {
									match (event_details.pallet_name(), event_details.variant_name()) {
										(ProxyAdded::PALLET, ProxyAdded::EVENT) => {
											Some(EventWrapper::ProxyAdded(event_details.as_event::<ProxyAdded>().unwrap().unwrap()))
										},
										(Transfer::PALLET, Transfer::EVENT) => {
											Some(EventWrapper::Transfer(event_details.as_event::<Transfer>().unwrap().unwrap()))
										},
										(TransactionFeePaid::PALLET, TransactionFeePaid::EVENT) => {
											Some(EventWrapper::TransactionFeePaid(event_details.as_event::<TransactionFeePaid>().unwrap().unwrap()))
										},
										_ => None,
									}.map(|event| (event_details.phase(), event))
								}
								Err(err) => {
									error!(
										"Error while parsing event: {:?}", err
									);
									None
								}
							}
						}).collect())
					}),
				);

				let mut end_at_block = None;
				let mut current_block = from_block;

				loop {
					let block_details = select! {
						end_block = &mut end_witnessing_receiver => {
							end_at_block = Some(end_block.expect("end witnessing channel was dropped unexpectedly"));
							None
						}
						Some((block_hash, block_number, block_event_details)) =	block_events_stream.next() => {
							current_block = block_number;
							Some((block_hash, block_number, block_event_details))
						}
					};

					if let Some(end_block) = end_at_block {
						if current_block >= end_block {
									info!("Polkadot block witnessers unsubscribe at block {end_block}");
									break
								}
							}

					if let Some((block_hash, block_number, block_event_details)) = block_details {
							trace!( "Checking block: {block_number}, with hash: {block_hash:?} for interesting events");
							let (
								interesting_indices,
								ingress_witnesses,
								vault_key_rotated_calls,
							) = check_for_interesting_events_in_block(
									block_event_details,
									block_number,
									&our_vault,
									&mut address_monitor,
									);

							for call in vault_key_rotated_calls {
								let _result = state_chain_client
										.submit_signed_extrinsic(
											pallet_cf_witnesser::Call::witness_at_epoch {
												call,
												epoch_index: epoch_start.epoch_index,
											},
										)
										.await;
							}

							if !interesting_indices.is_empty() {
								info!("We got an interesting block at block: {block_number}, hash: {block_hash:?}");

								let block = dot_client
								.block(block_hash)
								.await
								.context("Failed fetching block from DOT RPC")?
								.context(format!(
									"Polkadot block does not exist for block hash: {block_hash:?}",
								))?;

								while let Ok(sig) = signature_receiver.try_recv()
								{
									monitored_signatures.insert(sig);
								}
								for (extrinsic_index, tx_fee) in interesting_indices {
									let xt = block.extrinsics.get(extrinsic_index as usize).expect("We know this exists since we got this index from the event, from the block we are querying.");
									let xt_encoded = xt.0.encode();
									let mut xt_bytes = xt_encoded.as_slice();
									let unchecked = PolkadotUncheckedExtrinsic::decode(&mut xt_bytes);
									if let Ok(unchecked) = unchecked {
										let signature = unchecked.signature.unwrap().1;
										if let MultiSignature::Sr25519(sig) = signature {
											if monitored_signatures.contains(&sig.0) {
												info!("Witnessing signature_accepted. signature: {sig:?}");

												let _result = state_chain_client
													.submit_signed_extrinsic(
														pallet_cf_witnesser::Call::witness_at_epoch {
															call: Box::new(
																pallet_cf_broadcast::Call::<_, PolkadotInstance>::signature_accepted {
																	signature: sig.clone(),
																	signer_id: our_vault.clone(),
																	tx_fee
																}
																.into(),
															),
															epoch_index: epoch_start.epoch_index,
														},
													)
													.await;

											monitored_signatures.remove(&sig.0);
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

							if !ingress_witnesses.is_empty() {
								let _result =
									state_chain_client
										.submit_signed_extrinsic(
											pallet_cf_witnesser::Call::witness_at_epoch {
												call:
													Box::new(
														pallet_cf_ingress_egress::Call::<
															_,
															PolkadotInstance,
														>::do_ingress {
															ingress_witnesses,
														}
														.into(),
													),
												epoch_index: epoch_start.epoch_index,
											},
										)
										.await;
							}

							witnessed_until_sender
							.send(WitnessedUntil {
								epoch_index: epoch_start.epoch_index,
								block_number: block_number as u64,
							})
							.await
							.unwrap();
					}
				}
				Ok((address_monitor, monitored_signatures, signature_receiver))
			}
		},
	).instrument(info_span!("Dot-Witnesser"))
	.await
}

#[cfg(test)]
mod tests {

	use std::str::FromStr;

	use super::*;

	use cf_chains::dot;
	use cf_primitives::PolkadotAccountId;

	use crate::{
		dot::rpc::DotRpcClient, state_chain_observer::client::mocks::MockStateChainClient,
		witnesser::AddressMonitorCommand,
	};

	fn mock_proxy_added(
		delegator: &PolkadotAccountId,
		delegatee: &PolkadotAccountId,
	) -> EventWrapper {
		EventWrapper::ProxyAdded(ProxyAdded {
			delegator: delegator.clone(),
			delegatee: delegatee.clone(),
			proxy_type: PolkadotProxyType::Any,
			delay: 0,
		})
	}

	fn mock_tx_fee_paid(actual_fee: PolkadotBalance) -> EventWrapper {
		EventWrapper::TransactionFeePaid(TransactionFeePaid {
			actual_fee,
			who: PolkadotAccountId::from([0xab; 32]),
			tip: Default::default(),
		})
	}

	fn mock_transfer(
		from: &PolkadotAccountId,
		to: &PolkadotAccountId,
		amount: PolkadotBalance,
	) -> EventWrapper {
		EventWrapper::Transfer(Transfer { from: from.clone(), to: to.clone(), amount })
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
		let our_vault = PolkadotAccountId::from([0; 32]);
		let other_acct = PolkadotAccountId::from([1; 32]);
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

		let (mut interesting_indices, ingress_witnesses, vault_key_rotated_calls) =
			check_for_interesting_events_in_block(
				block_event_details,
				Default::default(),
				&our_vault,
				&mut AddressMonitor::new(Default::default()).1,
			);

		assert_eq!(vault_key_rotated_calls.len(), 1);
		assert_eq!(interesting_indices.pop().unwrap(), (our_proxy_added_index, fee_paid));
		assert!(ingress_witnesses.is_empty());
	}

	#[test]
	fn witness_ingresses_for_addresses_we_monitor() {
		// we want two monitors, one sent through at start, and one sent through channel
		const TRANSFER_1_INDEX: u32 = 1;
		let transfer_1_ingress_addr = PolkadotAccountId::from([1; 32]);
		const TRANSFER_1_AMOUNT: PolkadotBalance = 10000;

		const TRANSFER_2_INDEX: u32 = 2;
		let transfer_2_ingress_addr = PolkadotAccountId::from([2; 32]);
		const TRANSFER_2_AMOUNT: PolkadotBalance = 20000;

		let block_event_details = phase_and_events(&[
			// we'll be witnessing this from the start
			(
				TRANSFER_1_INDEX,
				mock_transfer(
					&PolkadotAccountId::from([7; 32]),
					&transfer_1_ingress_addr,
					TRANSFER_1_AMOUNT,
				),
			),
			// we'll receive this address from the channel
			(
				TRANSFER_2_INDEX,
				mock_transfer(
					&PolkadotAccountId::from([7; 32]),
					&transfer_2_ingress_addr,
					TRANSFER_2_AMOUNT,
				),
			),
			// this one is not for us
			(
				19,
				mock_transfer(
					&PolkadotAccountId::from([7; 32]),
					&PolkadotAccountId::from([9; 32]),
					93232,
				),
			),
		]);

		let (monitor_ingress_sender, mut address_monitor) =
			AddressMonitor::new(BTreeSet::from([transfer_1_ingress_addr]));

		monitor_ingress_sender
			.send(AddressMonitorCommand::Add(transfer_2_ingress_addr))
			.unwrap();

		let (interesting_indices, ingress_witnesses, vault_key_rotated_calls) =
			check_for_interesting_events_in_block(
				block_event_details,
				20,
				// arbitrary, not focus of the test
				&PolkadotAccountId::from([0xda; 32]),
				&mut address_monitor,
			);

		assert_eq!(ingress_witnesses.len(), 2);
		assert_eq!(ingress_witnesses.get(0).unwrap().amount, TRANSFER_1_AMOUNT);
		assert_eq!(ingress_witnesses.get(1).unwrap().amount, TRANSFER_2_AMOUNT);

		// We don't need to submit signature accepted for ingress witnesses
		assert_eq!(interesting_indices.len(), 0);
		assert!(vault_key_rotated_calls.is_empty());
	}

	#[test]
	fn ingress_fetch_and_egress_witnessed() {
		let egress_index = 3;
		let egress_amount = 30000;

		let ingress_fetch_index = 4;
		let ingress_fetch_amount = 40000;
		let our_vault = PolkadotAccountId::from([3; 32]);

		let block_event_details = phase_and_events(&[
			// we'll be witnessing this from the start
			(
				egress_index,
				// egress, from our vault
				mock_transfer(&our_vault, &PolkadotAccountId::from([6; 32]), egress_amount),
			),
			// fee same as amount for simpler testing
			(egress_index, mock_tx_fee_paid(egress_amount)),
			// we'll receive this address from the channel
			(
				ingress_fetch_index,
				// ingress fetch, to our vault
				mock_transfer(&PolkadotAccountId::from([7; 32]), &our_vault, ingress_fetch_amount),
			),
			(ingress_fetch_index, mock_tx_fee_paid(ingress_fetch_amount)),
			// this one is not for us
			(
				19,
				mock_transfer(
					&PolkadotAccountId::from([7; 32]),
					&PolkadotAccountId::from([9; 32]),
					93232,
				),
			),
		]);

		let (interesting_indices, ingress_witnesses, vault_key_rotated_calls) =
			check_for_interesting_events_in_block(
				block_event_details,
				20,
				// arbitrary, not focus of the test
				&our_vault,
				&mut AddressMonitor::new(BTreeSet::default()).1,
			);

		assert!(
			interesting_indices.contains(&(egress_index, egress_amount)) &&
				interesting_indices.contains(&(ingress_fetch_index, ingress_fetch_amount))
		);

		assert!(vault_key_rotated_calls.is_empty());
		assert!(ingress_witnesses.is_empty());
	}

	#[ignore = "This test is helpful for local testing. Requires connection to westend"]
	#[tokio::test]
	async fn start_witnessing() {
		let url = "ws://localhost:9944";

		println!("Connecting to: {url}");
		let dot_rpc_client = DotRpcClient::new(url).await.unwrap();

		let (epoch_starts_sender, epoch_starts_receiver) = async_broadcast::broadcast(10);

		let (dot_monitor_signature_sender, signature_receiver) =
			tokio::sync::mpsc::unbounded_channel();

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
		// dot_monitor_ingress_sender
		// 	.send(
		// 		PolkadotAccountId::from_str("5DJVVEYPDFZjj9JtJRE2vGvpeSnzBAUA74VXPSpkGKhJSHbN")
		// 			.unwrap(),
		// 	)
		// 	.unwrap();

		let signature: [u8; 64] = hex::decode("7c388203aefbcdc22077ed91bec9af80a23c56f8ff2ee24d40f4c2791d51773342f4aed0e8f0652ed33d404d9b78366a927be9fad02f5204f2f2ffbea7459886").unwrap().try_into().unwrap();

		dot_monitor_signature_sender.send(signature).unwrap();

		// proxy type governance
		epoch_starts_sender
			.broadcast(EpochStart {
				epoch_index: 1,
				block_number: 0,
				current: true,
				participant: true,
				data: dot::EpochStartData {
					vault_account: PolkadotAccountId::from_str(
						"12RtzLB2z2dsg9RUGtTuxhuyzsTFScYG8hBT7hJcaNbFqCry",
					)
					.unwrap(),
				},
			})
			.await
			.unwrap();

		let (_dir, db_path) = crate::testing::new_temp_directory_with_nonexistent_file();
		let db = PersistentKeyDB::open_and_migrate_to_latest(&db_path, None).unwrap();

		start(
			epoch_starts_receiver,
			dot_rpc_client,
			AddressMonitor::new(Default::default()).1,
			signature_receiver,
			BTreeSet::default(),
			state_chain_client,
			Arc::new(db),
		)
		.await
		.unwrap();
	}
}
