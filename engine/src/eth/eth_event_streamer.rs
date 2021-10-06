use anyhow::Result;
use futures::TryStreamExt;

use tokio_stream::{Stream, StreamExt};

use std::fmt::Debug;
use web3::{
    ethabi::RawLog,
    transports::WebSocket,
    types::{BlockNumber, FilterBuilder, Log, H160, H256, U64},
    Web3,
};

/// Creates a stream that outputs the events from a contract.
pub async fn new_eth_event_stream<
    Event: Debug,
    LogDecoder: Fn(H256, H256, RawLog) -> Result<Event>,
>(
    web3: &Web3<WebSocket>,
    deployed_address: H160,
    decode_log: LogDecoder,
    from_block: u64,
    logger: &slog::Logger,
) -> Result<impl Stream<Item = Result<Event>>, anyhow::Error> {
    // Start future log stream before requesting current block number, to ensure BlockNumber::Pending isn't after current_block
    let future_logs = web3
        .eth_subscribe()
        .subscribe_logs(
            FilterBuilder::default()
                .from_block(BlockNumber::Pending)
                .address(vec![deployed_address])
                .build(),
        )
        .await?;
    let from_block = U64::from(from_block);
    let current_block = web3.eth().block_number().await?;

    // The `fromBlock` parameter doesn't seem to work reliably with subscription streams, so
    // request past block via http and prepend them to the stream manually.
    let (past_logs, exclude_future_logs_before) = if from_block <= current_block {
        (
            web3.eth()
                .logs(
                    FilterBuilder::default()
                        .from_block(BlockNumber::Number(from_block))
                        .to_block(BlockNumber::Number(current_block))
                        .address(vec![deployed_address])
                        .build(),
                )
                .await?,
            current_block + 1,
        )
    } else {
        (vec![], from_block)
    };

    let future_logs =
        future_logs
            .map_err(anyhow::Error::new)
            .filter_map(move |result_unparsed_log| {
                // Need to remove logs that have already been included in past_logs or are before from_block
                match result_unparsed_log {
                    Ok(Log {
                        block_number: None, ..
                    }) => Some(Err(anyhow::Error::msg("Found log without block number"))),
                    Ok(Log {
                        block_number: Some(block_number),
                        ..
                    }) if block_number < exclude_future_logs_before => None,
                    _ => Some(result_unparsed_log),
                }
            });

    slog::info!(logger, "Future logs fetched");
    let logger = logger.clone();
    Ok(tokio_stream::iter(past_logs)
        .map(Ok)
        .chain(future_logs)
        .map(move |result_unparsed_log| -> Result<Event, anyhow::Error> {
            let result_event = result_unparsed_log.and_then(|log| {
                decode_log(
                    *log.topics.first().ok_or_else(|| {
                        anyhow::Error::msg("Could not get event signature from ETH log")
                    })?,
                    log.transaction_hash.ok_or_else(|| {
                        anyhow::Error::msg("Could not get transaction hash from ETH log")
                    })?,
                    RawLog {
                        topics: log.topics,
                        data: log.data.0,
                    },
                )
            });

            slog::debug!(
                logger,
                "Received ETH log, parsing result: {:?}",
                result_event
            );

            result_event
        }))
}

#[cfg(test)]
mod tests {

    use crate::{
        eth::{key_manager::KeyManager, new_synced_web3_client},
        logging, settings,
    };

    use super::*;

    #[tokio::test]
    #[ignore = "Depends on a running ganache instance, runs forever, useful for manually testing / observing incoming events"]
    async fn subscribe_to_key_manager_events() {
        let logger = logging::test_utils::create_test_logger();

        let settings = settings::test_utils::new_test_settings().unwrap();

        let key_manager = KeyManager::new(&settings).unwrap();

        key_manager
            .event_stream(
                &new_synced_web3_client(&settings, &logger).await.unwrap(),
                settings.eth.from_block,
                &logger,
            )
            .await
            .unwrap()
            .collect::<Vec<_>>()
            .await;
    }
}
