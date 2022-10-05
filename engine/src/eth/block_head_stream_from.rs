use std::pin::Pin;

use futures::{stream, Stream};
use web3::types::U64;

use super::{rpc::EthRpcApi, EthNumberBloom};
use futures::StreamExt;

use anyhow::{anyhow, Context, Result};

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

            let past_heads = Box::pin(
                stream::iter(from_block.as_u64()..=best_safe_block_number.as_u64()).then(
                    move |block_number| {
                        let eth_rpc = eth_rpc.clone();
                        async move {
                            eth_rpc
                                .block(U64::from(block_number))
                                .await
                                .and_then(|block| {
                                    let number_bloom: Result<EthNumberBloom> = block
                                        .try_into()
                                        .context("Failed to convert Block to EthNumberBloom");
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

#[cfg(test)]
mod tests {
    use sp_core::H256;
    use web3::types::Block;

    use crate::{eth::rpc::mocks::MockEthHttpRpcClient, logging::test_utils::new_test_logger};

    use super::*;

    fn block(block_number: U64) -> Result<Block<H256>> {
        Ok(Block {
            number: Some(block_number),
            logs_bloom: Some(Default::default()),
            base_fee_per_gas: Some(Default::default()),
            ..Default::default()
        })
    }

    // We don't care about the logs_bloom or base_fee_per_gas for these tests
    fn number_bloom(block_number: u64) -> EthNumberBloom {
        EthNumberBloom {
            block_number: U64::from(block_number),
            logs_bloom: Default::default(),
            base_fee_per_gas: Default::default(),
        }
    }

    #[tokio::test]
    async fn stream_does_not_begin_yielding_until_at_from_block() {
        let logger = new_test_logger();

        let inner_stream_starts_at = 10;
        let from_block = 15;
        let inner_stream_ends_at = 20;

        // .block should not be called on the RPC returned from here
        let mut mock_eth_rpc = MockEthHttpRpcClient::new();
        let mut mock_eth_rpc2 = MockEthHttpRpcClient::new();
        mock_eth_rpc2.expect_block().returning(|n| block(n));
        mock_eth_rpc.expect_clone().return_once(|| mock_eth_rpc2);

        let safe_head_stream = stream::iter(
            (inner_stream_starts_at..inner_stream_ends_at).map(|number| number_bloom(number)),
        );

        let mut safe_head_stream_from =
            block_head_stream_from(from_block, safe_head_stream, mock_eth_rpc, &logger)
                .await
                .unwrap();

        // We should only be yielding from the `from_block`
        for expected_block_number in from_block..inner_stream_ends_at {
            assert_eq!(
                safe_head_stream_from
                    .next()
                    .await
                    .unwrap()
                    .block_number
                    .as_u64(),
                expected_block_number
            );
        }

        assert!(safe_head_stream_from.next().await.is_none());
    }
}
