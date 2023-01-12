#![cfg(feature = "ibiza")]

use std::{collections::BTreeSet, pin::Pin, sync::Arc};

use cf_chains::eth::Ethereum;
use cf_primitives::chains::assets::eth;
use futures::Stream;
use pallet_cf_ingress_egress::IngressWitness;
use state_chain_runtime::EthereumInstance;
use tokio_stream::StreamExt;
use web3::types::Transaction;

use crate::{
	eth::{core_h160, core_h256},
	state_chain_observer::client::extrinsic_api::ExtrinsicApi,
	witnesser::{
		checkpointing::{start_checkpointing_for, WitnessedUntil},
		epoch_witnesser::{self, should_end_witnessing},
		EpochStart,
	},
};

use super::{
	eth_block_head_stream_from,
	http_safe_stream::{safe_polling_http_head_stream, HTTP_POLL_INTERVAL},
	merged_block_items_stream,
	rpc::{EthDualRpcClient, EthRpcApi, EthWsRpcApi},
	ws_safe_stream::safe_ws_head_stream,
	BlockWithProcessedItems, EthNumberBloom,
};

use anyhow::Result;

use crate::eth::ETH_BLOCK_SAFETY_MARGIN;

async fn block_transactions_stream_from_head_stream<BlockHeaderStream, EthRpc>(
	from_block: u64,
	safe_head_stream: BlockHeaderStream,
	eth_rpc: EthRpc,
	logger: &slog::Logger,
) -> Result<Pin<Box<dyn Stream<Item = BlockWithProcessedItems<Transaction>> + Send + 'static>>>
where
	BlockHeaderStream: Stream<Item = EthNumberBloom> + 'static + Send,
	EthRpc: 'static + EthRpcApi + Send + Sync + Clone,
{
	Ok(Box::pin(
		eth_block_head_stream_from(from_block, safe_head_stream, eth_rpc.clone(), logger)
			.await?
			.then(move |header| {
				let eth_rpc = eth_rpc.clone();
				async move {
					BlockWithProcessedItems {
						block_number: header.block_number.as_u64(),
						processed_block_items: eth_rpc
							.block_with_txs(header.block_number)
							.await
							.map(|block| block.transactions),
					}
				}
			}),
	))
}

// NB: This code can emit the same witness multiple times. e.g. if the CFE restarts in the middle of
// witnessing a window of blocks
pub async fn start<StateChainClient>(
	eth_dual_rpc: EthDualRpcClient,
	epoch_starts_receiver: async_broadcast::Receiver<EpochStart<Ethereum>>,
	eth_monitor_ingress_receiver: tokio::sync::mpsc::UnboundedReceiver<sp_core::H160>,
	state_chain_client: Arc<StateChainClient>,
	monitored_addresses: BTreeSet<sp_core::H160>,
	logger: &slog::Logger,
) -> anyhow::Result<()>
where
	StateChainClient: ExtrinsicApi + 'static + Send + Sync,
{
	epoch_witnesser::start(
		"ETH-Ingress".to_string(),
		epoch_starts_receiver,
		|_epoch_start| true,
		(monitored_addresses, eth_monitor_ingress_receiver),
		move |end_witnessing_signal,
		      epoch_start,
		      (mut monitored_addresses, mut eth_monitor_ingress_receiver),
		      logger| {
			let eth_ws_rpc = eth_dual_rpc.ws_client.clone();
			let eth_http_rpc = eth_dual_rpc.http_client.clone();
			let state_chain_client = state_chain_client.clone();
			async move {

				let (witnessed_until, witnessed_until_sender) = start_checkpointing_for("eth-ingress", &logger).await;

				// Don't witness for past epochs
				if epoch_start.epoch_index < witnessed_until.epoch_index {
					return Ok((monitored_addresses, eth_monitor_ingress_receiver));
				}

				let from_block = if witnessed_until.epoch_index == epoch_start.epoch_index {
					std::cmp::max(epoch_start.block_number, witnessed_until.block_number)
				} else {
					epoch_start.block_number
				};

				let safe_ws_tx_stream = block_transactions_stream_from_head_stream(
					from_block,
					safe_ws_head_stream(
						eth_ws_rpc.subscribe_new_heads().await?,
						ETH_BLOCK_SAFETY_MARGIN,
						&logger,
					),
					eth_ws_rpc.clone(),
					&logger,
				)
				.await?;

				let safe_http_tx_stream = block_transactions_stream_from_head_stream(
					from_block,
					safe_polling_http_head_stream(
						eth_http_rpc.clone(),
						HTTP_POLL_INTERVAL,
						ETH_BLOCK_SAFETY_MARGIN,
						&logger,
					)
					.await,
					eth_http_rpc.clone(),
					&logger,
				)
				.await?;

				let mut merged_stream = merged_block_items_stream(
					safe_ws_tx_stream,
					safe_http_tx_stream,
					logger.clone(),
				)
				.await?;

				loop {
					tokio::select! {
						// We want to bias the select so we check new addresses to monitor before we check the addresses
						// ensuring we don't potentially miss any ingress events that occur before we start to monitor the address
						biased;
						Some(to_monitor) = eth_monitor_ingress_receiver.recv() => {
							monitored_addresses.insert(to_monitor);
						},
						Some(block_with_txs) = merged_stream.next() => {

							if should_end_witnessing::<Ethereum>(end_witnessing_signal.clone(), block_with_txs.block_number, &logger) {
								break;
							}

							let ingress_witnesses = block_with_txs.block_items
								.iter()
								.filter_map(|tx| {
									let to_addr = tx.to?;
									if monitored_addresses.contains(&core_h160(to_addr)) {
										Some((tx, to_addr))
									} else {
										None
									}
								}).map(|(tx, to_addr)| {
									IngressWitness {
										ingress_address: core_h160(to_addr),
										asset: eth::Asset::Eth,
										amount: tx.value.as_u128(),
										tx_id: core_h256(tx.hash)
									}
								})
								.collect::<Vec<IngressWitness<Ethereum>>>();

								if !ingress_witnesses.is_empty() {
									let _result = state_chain_client
										.submit_signed_extrinsic(
											pallet_cf_witnesser::Call::witness_at_epoch {
												call: Box::new(
													pallet_cf_ingress_egress::Call::<_, EthereumInstance>::do_ingress {
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

								witnessed_until_sender
									.send(WitnessedUntil {
										epoch_index: epoch_start.epoch_index,
										block_number: block_with_txs.block_number,
									})
									.unwrap();

						},
						else => break,
					};
				}

				Ok((monitored_addresses, eth_monitor_ingress_receiver))
			}
		},
		logger,
	)
	.await
}
