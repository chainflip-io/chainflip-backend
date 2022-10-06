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
    use async_trait::async_trait;
    use sp_core::{H256, U256};
    use web3::types::{
        Block, BlockNumber, Bytes, CallRequest, FeeHistory, Filter, Log, SignedTransaction,
        Transaction, TransactionParameters, TransactionReceipt,
    };
    use web3_secp256k1::SecretKey;

    use crate::logging::test_utils::new_test_logger;

    use super::*;

    #[derive(Clone)]
    struct MockEthRpc {}

    #[async_trait]
    impl EthRpcApi for MockEthRpc {
        async fn estimate_gas(
            &self,
            _req: CallRequest,
            _block: Option<BlockNumber>,
        ) -> Result<U256> {
            unimplemented!("not used");
        }

        async fn sign_transaction(
            &self,
            _tx: TransactionParameters,
            _key: &SecretKey,
        ) -> Result<SignedTransaction> {
            unimplemented!("not used");
        }

        async fn send_raw_transaction(&self, _rlp: Bytes) -> Result<H256> {
            unimplemented!("not used");
        }

        async fn get_logs(&self, _filter: Filter) -> Result<Vec<Log>> {
            unimplemented!("not used");
        }

        async fn chain_id(&self) -> Result<U256> {
            unimplemented!("not used");
        }

        async fn transaction_receipt(&self, _tx_hash: H256) -> Result<TransactionReceipt> {
            unimplemented!("not used");
        }

        async fn block(&self, block_number: U64) -> Result<Block<H256>> {
            Ok(Block {
                number: Some(block_number),
                logs_bloom: Some(Default::default()),
                base_fee_per_gas: Some(Default::default()),
                ..Default::default()
            })
        }

        async fn block_with_txs(&self, _block_number: U64) -> Result<Block<Transaction>> {
            unimplemented!("not used");
        }

        async fn fee_history(
            &self,
            _block_count: U256,
            _newest_block: BlockNumber,
            _reward_percentiles: Option<Vec<f64>>,
        ) -> Result<FeeHistory> {
            unimplemented!("not used");
        }

        async fn block_number(&self) -> Result<U64> {
            unimplemented!("not used");
        }
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

        let safe_head_stream = stream::iter(
            (inner_stream_starts_at..inner_stream_ends_at).map(|number| number_bloom(number)),
        );

        let mut safe_head_stream_from =
            block_head_stream_from(from_block, safe_head_stream, MockEthRpc {}, &logger)
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

    #[tokio::test]
    async fn stream_goes_back_if_inner_stream_starts_ahead_of_from_block() {
        let logger = new_test_logger();

        let from_block = 10;
        let inner_stream_starts_at = 15;
        let inner_stream_ends_at = 20;

        let safe_head_stream = stream::iter(
            (inner_stream_starts_at..inner_stream_ends_at).map(|number| number_bloom(number)),
        );

        let mut safe_head_stream_from =
            block_head_stream_from(from_block, safe_head_stream, MockEthRpc {}, &logger)
                .await
                .unwrap();

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
