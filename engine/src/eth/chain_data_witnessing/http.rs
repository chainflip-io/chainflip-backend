use std::time::Duration;

use super::tick_stream;

use crate::{eth::rpc::EthRpcApi, logging::COMPONENT_KEY};

use futures::{future, stream::BoxStream, StreamExt};
use slog::o;

/// Returns a stream of latest eth block numbers by polling at regular intervals.
pub fn latest_block_numbers<'a, HttpRpc: EthRpcApi + Send + Sync>(
    eth_http_rpc: &'a HttpRpc,
    polling_interval: Duration,
    logger: &slog::Logger,
) -> BoxStream<'a, u64> {
    let logger = logger.new(o!(COMPONENT_KEY => "ETH_HTTP_LatestBlockStream"));

    Box::pin(
        tick_stream(polling_interval)
            // Get the latest block number.
            .then(move |_| async move { eth_http_rpc.block_number().await })
            // Warn on error.
            .filter_map(move |rpc_result| {
                future::ready(match rpc_result {
                    Ok(block_number) => Some(block_number.as_u64()),
                    Err(e) => {
                        slog::warn!(logger, "Error fetching ETH block number: {}", e);
                        None
                    }
                })
            })
            // Deduplicate block numbers.
            .scan(0, |last, latest| {
                future::ready(Some(if *last != latest {
                    *last = latest;
                    Some(latest)
                } else {
                    None
                }))
            })
            // Unwrap, ignoring None values.
            .filter_map(future::ready),
    )
}
