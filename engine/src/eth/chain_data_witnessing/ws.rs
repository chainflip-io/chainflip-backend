use crate::{eth::rpc::EthWsRpcApi, logging::COMPONENT_KEY};
use futures::{future, stream::BoxStream, StreamExt};
use slog::o;

/// Returns a stream of latest eth block numbers.
pub async fn latest_block_numbers<'a, WsRpc: EthWsRpcApi + Send + Sync>(
    eth_ws_rpc: &'a WsRpc,
    logger: &slog::Logger,
) -> anyhow::Result<BoxStream<'a, u64>> {
    let logger = logger.new(o!(COMPONENT_KEY => "ETH_WS_LatestBlockStream"));

    Ok(Box::pin(
        eth_ws_rpc
            .subscribe_new_heads()
            .await?
            .filter_map(move |rpc_result| {
                future::ready(match rpc_result {
                    Ok(header) => header.number.map(|n| n.as_u64()),
                    Err(e) => {
                        slog::warn!(logger, "Error fetching ETH block number: {}", e);
                        None
                    }
                })
            }),
    ))
}
