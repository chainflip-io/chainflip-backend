use std::sync::Arc;

use cf_primitives::{Asset, ForeignChainAddress};
use sp_core::H160;
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use web3::types::Transaction;

use crate::state_chain_observer::client::{StateChainClient, StateChainRpcApi};

use super::{
    epoch_witnesser,
    rpc::{EthRpcApi, EthWsRpcApi, EthWsRpcClient},
    ws_safe_stream::safe_ws_head_stream,
    EpochStart,
};

use crate::eth::ETH_BLOCK_SAFETY_MARGIN;

// NB: This code can emit the same witness multiple times. e.g. if the CFE restarts in the middle of witnessing a window of blocks
pub async fn start<StateChainRpc>(
    // TODO: Add HTTP client and merged stream functionality for redundancy
    eth_ws_rpc: EthWsRpcClient,
    epoch_starts_receiver: broadcast::Receiver<EpochStart>,
    state_chain_client: Arc<StateChainClient<StateChainRpc>>,
    monitored_addresses: Vec<H160>,
    logger: &slog::Logger,
) -> anyhow::Result<()>
where
    StateChainRpc: 'static + StateChainRpcApi + Sync + Send,
{
    epoch_witnesser::start(
        "ETH-Ingress-Witnesser",
        epoch_starts_receiver,
        |_epoch_start| true,
        monitored_addresses,
        move |end_witnessing_signal, epoch_start, monitored_addresses, logger| {
            let eth_ws_rpc = eth_ws_rpc.clone();
            let state_chain_client = state_chain_client.clone();
            async move {
                slog::info!(
                    logger,
                    "Start witnessing from ETH block: {}",
                    epoch_start.eth_block
                );

                // TODO: Factor out merged streams for use in contract witnesser and here
                let mut safe_ws_head_stream = safe_ws_head_stream(
                    eth_ws_rpc.subscribe_new_heads().await?,
                    ETH_BLOCK_SAFETY_MARGIN,
                    &logger,
                );

                // TODO: Use async channel to receive updates to the montiored addresses

                // select! in a biased mode, we want to get the latest monitored addresses before continuing to see if we have received any
                // witnesses for them

                while let Some(number_bloom) = safe_ws_head_stream.next().await {
                    // TODO: Factor out ending between contract witnesser and ingress witnesser
                    if let Some(end_block) = *end_witnessing_signal.lock().unwrap() {
                        if number_bloom.block_number.as_u64() >= end_block {
                            slog::info!(
                                logger,
                                "Finished witnessing events at ETH block: {}",
                                number_bloom.block_number
                            );
                            // we have reached the block height we wanted to witness up to
                            // so can stop the witness process
                            break;
                        }
                    }

                    for (tx, to_addr) in eth_ws_rpc
                        .block_with_txs(number_bloom.block_number)
                        .await?
                        .transactions
                        .iter()
                        .filter_map(|tx| {
                            let to_addr = tx.to?;
                            if monitored_addresses.contains(&to_addr) {
                                Some((tx, to_addr))
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<(&Transaction, H160)>>()
                    {
                        let _result = state_chain_client
                            .submit_signed_extrinsic(
                                pallet_cf_witnesser::Call::witness_at_epoch {
                                    call: Box::new(
                                        pallet_cf_ingress::Call::do_ingress {
                                            address: ForeignChainAddress::Eth(to_addr),
                                            asset: Asset::Eth,
                                            amount: tx.value.as_u128(),
                                            tx_hash: tx.hash,
                                        }
                                        .into(),
                                    ),
                                    epoch_index: epoch_start.index,
                                },
                                &logger,
                            )
                            .await;
                    }
                }

                Ok(monitored_addresses)
            }
        },
        logger,
    )
    .await
}
