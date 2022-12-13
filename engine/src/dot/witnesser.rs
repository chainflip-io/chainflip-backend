use std::{collections::BTreeSet, pin::Pin, sync::Arc};

use cf_chains::{
	dot::{
		Polkadot, PolkadotBalance, PolkadotBlockNumber, PolkadotHash, PolkadotProxyType,
		PolkadotUncheckedExtrinsic, TxId,
	},
	eth::assets,
};
use cf_primitives::PolkadotAccountId;
use codec::{Decode, Encode};
use frame_support::scale_info::TypeInfo;
use futures::{stream, Stream, StreamExt};
use pallet_cf_ingress_egress::IngressWitness;
use sp_runtime::MultiSignature;
use state_chain_runtime::PolkadotInstance;
use subxt::{
	events::{EventFilter, EventsClient, Phase, StaticEvent},
	OnlineClient, PolkadotConfig,
};

use crate::{
	state_chain_observer::client::extrinsic_api::ExtrinsicApi,
	witnesser::{
		block_head_stream_from::block_head_stream_from, epoch_witnesser, BlockNumberable,
		EpochStart,
	},
};

use anyhow::{Context, Result};

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

pub async fn dot_block_head_stream_from<BlockHeaderStream>(
	from_block: PolkadotBlockNumber,
	safe_head_stream: BlockHeaderStream,
	dot_client: OnlineClient<PolkadotConfig>,
	logger: &slog::Logger,
) -> Result<Pin<Box<dyn Stream<Item = MiniHeader> + Send + 'static>>>
where
	BlockHeaderStream: Stream<Item = MiniHeader> + 'static + Send,
{
	block_head_stream_from(
		from_block,
		safe_head_stream,
		move |block_number| {
			let dot_client = dot_client.clone();
			Box::pin(async move {
				let block_hash = dot_client
					.rpc()
					.block_hash(Some(block_number.into()))
					.await?
					.expect("Called on a finalised stream, so the block will exist");
				Ok(MiniHeader { block_number, block_hash })
			})
		},
		logger,
	)
	.await
}

/// Takes a stream of Results and terminates when it hits an error, logging the error before
/// terminating.
fn take_while_ok<InStream, T, E>(
	inner_stream: InStream,
	logger: &slog::Logger,
) -> impl Stream<Item = T>
where
	InStream: Stream<Item = std::result::Result<T, E>>,
	E: std::fmt::Debug,
{
	struct StreamState<FromStream, T, E>
	where
		FromStream: Stream<Item = std::result::Result<T, E>>,
	{
		stream: FromStream,
		logger: slog::Logger,
	}

	let init_state = StreamState { stream: Box::pin(inner_stream), logger: logger.clone() };

	stream::unfold(init_state, move |mut state| async move {
		match state.stream.next().await {
			Some(Ok(item)) => Some((item, state)),
			Some(Err(err)) => {
				slog::error!(&state.logger, "Error on stream: {:?}", err);
				None
			},
			None => None,
		}
	})
}

pub async fn start<StateChainClient>(
	epoch_starts_receiver: async_broadcast::Receiver<EpochStart<Polkadot>>,
	dot_client: OnlineClient<PolkadotConfig>,
	dot_monitor_ingress_receiver: tokio::sync::mpsc::UnboundedReceiver<PolkadotAccountId>,
	monitored_addresses: BTreeSet<PolkadotAccountId>,
	state_chain_client: Arc<StateChainClient>,
	logger: &slog::Logger,
) -> Result<()>
where
	StateChainClient: ExtrinsicApi + 'static + Send + Sync,
{
	epoch_witnesser::start(
		"DOT".to_string(),
		epoch_starts_receiver,
		|_epoch_start| true,
		(monitored_addresses, dot_monitor_ingress_receiver),
		move |_end_witnessing_signal, epoch_start, (mut monitored_addresses, mut dot_monitor_ingress_receiver), logger| {
			let dot_client = dot_client.clone();
			let state_chain_client = state_chain_client.clone();
			async move {
				let safe_head_stream = take_while_ok(dot_client
					.rpc()
					.subscribe_finalized_blocks()
					.await?,
					&logger)
					.map(|header| {
						MiniHeader { block_number: header.number, block_hash: header.hash() }
					});

				let block_head_stream_from = dot_block_head_stream_from(
					epoch_start.block_number,
					safe_head_stream,
					dot_client.clone(),
					&logger,
				)
				.await?;

				let our_vault = epoch_start.data.vault_account;

				// Stream of Events objects. Each `Events` contains the events for a particular
				// block
				let mut filtered_events_stream = Box::pin(
					take_while_ok(block_head_stream_from
						.then(|mini_header| {
							let dot_client = dot_client.clone();
							slog::info!(&logger, "Fetching Polkadot events for block: {}", mini_header.block_number);
							// TODO: This will not work if the block we are querying metadata has
							// different metadata than the latest block since this client fetches
							// the latest metadata and always uses it.
							// https://github.com/chainflip-io/chainflip-backend/issues/2542
							async move {
								let events = EventsClient::new(dot_client).at(Some(mini_header.block_hash)).await?;
								Result::<_, anyhow::Error>::Ok((mini_header.block_number, events))
							}
						}), &logger)
						.map(|(block_number, events)| {
							(block_number, <(ProxyAdded, Transfer)>::filter(events))
						}),
				);

				while let Some((block_number, mut block_event_details)) = filtered_events_stream.next().await {
					// to contain all the ingress witnessse for this block
					let mut ingress_witnesses = Vec::new();
					while let Some(Ok(event_details)) = block_event_details.next() {

						match event_details.event {
							(Some(ProxyAdded { delegator, .. }), None) => {
								if AsRef::<[u8; 32]>::as_ref(&delegator) != AsRef::<[u8; 32]>::as_ref(&our_vault) {
									continue
								}

								if let Phase::ApplyExtrinsic(extrinsic_index) = event_details.phase {
									let block = dot_client
										.rpc()
										.block(Some(event_details.block_hash))
										.await
										.context("Failed fetching block from DOT RPC")?
										.context(format!(
											"Polkadot block does not exist for block hash: {:?}",
											event_details.block_hash,
										))?;

									let xt = block.block.extrinsics.get(extrinsic_index as usize).expect("We know this exists since we got this index from the event, from the block we are querying.");
									let xt_encoded = xt.encode();
									let mut xt_bytes = xt_encoded.as_slice();
									let unchecked = PolkadotUncheckedExtrinsic::decode(&mut xt_bytes);
									if let Ok(unchecked) = unchecked {
										let signature = unchecked.signature.unwrap().1;
										if let MultiSignature::Sr25519(sig) = signature {
											slog::info!(
												logger,
												"Witnessing ProxyAdded {{ signature: {sig:?}, signer: {our_vault:?} }}"
											);
											let _result = state_chain_client
												.submit_signed_extrinsic(
													pallet_cf_witnesser::Call::witness_at_epoch {
														call: Box::new(
															pallet_cf_broadcast::Call::<
																_,
																PolkadotInstance,
															>::signature_accepted {
																signature: sig,
																signer_id: our_vault.clone(),
																// TODO: https://github.com/chainflip-io/chainflip-backend/issues/2544
																tx_fee: 1000,
															}
															.into(),
														),
														epoch_index: epoch_start.epoch_index,
													},
													&logger,
												)
												.await;
										} else {
											slog::error!(
												logger,
												"Signature not Sr25519. Got {:?} instead.",
												signature
											)
										}
									} else {
										slog::error!(
											logger,
											"Failed to decode UncheckedExtrinsic {:?}",
											unchecked
										);
									}
								}
							},
							(None, Some(Transfer { to, amount, .. })) => {

								// When we get a transfer event, we want to check that we have pulled the latest addresses to monitor from the chain first
								while let Ok(address) = dot_monitor_ingress_receiver.try_recv() {
									monitored_addresses.insert(address);
								}

								if monitored_addresses.contains(&to) {
									if let Phase::ApplyExtrinsic(extrinsic_index) = event_details.phase {
										slog::info!(logger, "Witnessing DOT Transfer {{ amount: {amount:?}, to: {to:?}");
										ingress_witnesses.push(IngressWitness {
											ingress_address: to,
											asset: assets::dot::Asset::Dot,
											amount,
											tx_id: TxId {
												block_number,
												extrinsic_index
											}
										})
									}
								}
							},
							(Some(_), Some(_)) => unreachable!("An event can only be one event at once."),
							_ => {
								// just not an interesting event
							}
						}
					}
					// We've finished iterating the events for this block
					if !ingress_witnesses.is_empty() {
						let _result = state_chain_client
							.submit_signed_extrinsic(
								pallet_cf_witnesser::Call::witness_at_epoch {
									call: Box::new(
										pallet_cf_ingress_egress::Call::<_, PolkadotInstance>::do_ingress {
											ingress_witnesses
										}
										.into(),
									),
									epoch_index: epoch_start.epoch_index,
								},
								&logger,
							)
							.await;
					}
				}
				Ok((monitored_addresses, dot_monitor_ingress_receiver))
			}
		},
		logger,
	)
	.await
}

#[cfg(test)]
mod tests {

	use std::str::FromStr;

	use super::*;

	use cf_chains::dot;
	use cf_primitives::PolkadotAccountId;
	use subxt::PolkadotConfig;

	use crate::{
		logging::test_utils::new_test_logger,
		state_chain_observer::client::mocks::MockStateChainClient,
	};

	#[ignore = "This test is helpful for local testing. Requires connection to westend"]
	#[tokio::test]
	async fn start_witnessing() {
		let url = "url";

		let logger = new_test_logger();

		println!("Connecting to: {}", url);
		let dot_client = OnlineClient::<PolkadotConfig>::from_url(url).await.unwrap();

		let client_metadata = dot_client.metadata();
		let client_types = client_metadata.types();
		// println!("Here's the current metadata: {:?}", client_metadata);

		let current_metadata = dot_client.rpc().metadata().await.unwrap();
		let current_types = current_metadata.types();
		assert_eq!(client_types, current_types);

		let (epoch_starts_sender, epoch_starts_receiver) = async_broadcast::broadcast(10);

		let (dot_monitor_ingress_sender, dot_monitor_ingress_receiver) =
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
		dot_monitor_ingress_sender
			.send(
				PolkadotAccountId::from_str("5DJVVEYPDFZjj9JtJRE2vGvpeSnzBAUA74VXPSpkGKhJSHbN")
					.unwrap(),
			)
			.unwrap();

		// proxy type governance
		epoch_starts_sender
			.broadcast(EpochStart {
				epoch_index: 3,
				block_number: 13658900,
				current: true,
				participant: true,
				data: dot::EpochStartData {
					vault_account: PolkadotAccountId::from_str(
						"5GC5yQrww6NJE11YeKEUoEChBL4Vydqq96xJSZjb8kc6Ru1H",
					)
					.unwrap(),
				},
			})
			.await
			.unwrap();

		start(
			epoch_starts_receiver,
			dot_client,
			dot_monitor_ingress_receiver,
			BTreeSet::default(),
			state_chain_client,
			&logger,
		)
		.await
		.unwrap();
	}
}
