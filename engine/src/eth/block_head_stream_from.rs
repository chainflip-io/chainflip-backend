use std::pin::Pin;

use futures::{stream, Stream};
use web3::types::U64;

use super::{rpc::EthRpcApi, EthNumberBloom};
use futures::StreamExt;

use anyhow::{anyhow, Result};

pub async fn block_head_stream_from<BlockHeaderStream, EthRpc>(
    from_block: u64,
    safe_head_stream: BlockHeaderStream,
    eth_rpc: EthRpc,
    logger: &slog::Logger,
) -> Result<Pin<Box<dyn Stream<Item = EthNumberBloom> + Send + 'static>>>
where
    BlockHeaderStream: Stream<Item = EthNumberBloom> + 'static + Send,
    EthRpc: 'static + EthRpcApi + Send + Sync + Clone,
{
    let from_block = U64::from(from_block);
    let mut safe_head_stream = Box::pin(safe_head_stream);
    // only allow pulling from the stream once we are actually at our from_block number
    while let Some(best_safe_block_header) = safe_head_stream.next().await {
        let best_safe_block_number = best_safe_block_header.block_number;
        // we only want to start witnessing once we reach the from_block specified
        if best_safe_block_number < from_block {
            slog::trace!(
                logger,
                "Not witnessing until ETH block `{}` Received block `{}` from stream.",
                from_block,
                best_safe_block_number
            );
        } else {
            // our chain_head is above the from_block number

            let eth_rpc_c = eth_rpc.clone();

            let past_heads = Box::pin(
                stream::iter(from_block.as_u64()..=best_safe_block_number.as_u64()).then(
                    move |block_number| {
                        let eth_rpc = eth_rpc_c.clone();
                        async move {
                            eth_rpc
                                .block(U64::from(block_number))
                                .await
                                .and_then(|block| {
                                    let number_bloom: Result<EthNumberBloom> = block.try_into();
                                    number_bloom
                                })
                        }
                    },
                ),
            );

            return Ok(Box::pin(
                stream::unfold(
                    (past_heads, safe_head_stream),
                    |(mut past_heads, mut safe_head_stream)| async {
                        // we want to consume the past logs stream first, terminating if any of these logs are an error
                        if let Some(result_past_log) = past_heads.next().await {
                            if let Ok(past_log) = result_past_log {
                                Some((past_log, (past_heads, safe_head_stream)))
                            } else {
                                None
                            }
                        } else {
                            // the past logs were consumed, now we consume the "future" logs
                            safe_head_stream
                                .next()
                                .await
                                .map(|future_log| (future_log, (past_heads, safe_head_stream)))
                        }
                    },
                )
                .fuse(),
            ));
        }
    }
    Err(anyhow!("No events in ETH safe head stream"))
}
