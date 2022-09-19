use std::{collections::BTreeSet, sync::Arc};

use cf_primitives::{Asset, ForeignChainAddress};
use pallet_cf_ingress::IngressWitness;
use sp_core::H160;
use tokio::sync::broadcast;
use tokio_stream::StreamExt;

use crate::{
    eth::epoch_witnesser::should_end_witnessing,
    state_chain_observer::client::{StateChainClient, StateChainRpcApi},
};

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
    eth_monitor_ingress_receiver: tokio::sync::mpsc::UnboundedReceiver<H160>,
    state_chain_client: Arc<StateChainClient<StateChainRpc>>,
    monitored_addresses: BTreeSet<H160>,
    logger: &slog::Logger,
) -> anyhow::Result<()>
where
    StateChainRpc: 'static + StateChainRpcApi + Sync + Send,
{
    epoch_witnesser::start(
        "ETH-Ingress-Witnesser",
        epoch_starts_receiver,
        |_epoch_start| true,
        (monitored_addresses, eth_monitor_ingress_receiver),
        move |end_witnessing_signal,
              epoch_start,
              (mut monitored_addresses, mut eth_monitor_ingress_receiver),
              logger| {
            let eth_ws_rpc = eth_ws_rpc.clone();
            let state_chain_client = state_chain_client.clone();
            async move {
                // TODO: Factor out merged streams for use in contract witnesser and here
                let mut safe_ws_head_stream = safe_ws_head_stream(
                    eth_ws_rpc.subscribe_new_heads().await?,
                    ETH_BLOCK_SAFETY_MARGIN,
                    &logger,
                );

                loop {
                    tokio::select! {
                        // We want to bias the select so we check new addresses to monitor before we check the addresses
                        // ensuring we don't potentially miss any ingress events that occur before we start to monitor the address
                        biased;
                        Some(to_monitor) = eth_monitor_ingress_receiver.recv() => {
                            monitored_addresses.insert(to_monitor);
                        },
                        Some(number_bloom) = safe_ws_head_stream.next() => {

                            if should_end_witnessing(end_witnessing_signal.clone(), number_bloom.block_number.as_u64(), &logger) {
                                break;
                            }

                            let ingress_witnesses = eth_ws_rpc
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
                                }).map(|(tx, to_addr)| {
                                    IngressWitness {
                                        ingress_address: ForeignChainAddress::Eth(
                                            to_addr.into(),
                                        ),
                                        asset: Asset::Eth,
                                        amount: tx.value.as_u128(),
                                        tx_hash: tx.hash
                                    }
                                })
                                .collect::<Vec<IngressWitness>>();

                                let _result = state_chain_client
                                    .submit_signed_extrinsic(
                                        pallet_cf_witnesser::Call::witness_at_epoch {
                                            call: Box::new(
                                                pallet_cf_ingress::Call::do_ingress {
                                                    ingress_witnesses
                                                }
                                                .into(),
                                            ),
                                            epoch_index: epoch_start.index,
                                        },
                                        &logger,
                                    )
                                    .await;
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
