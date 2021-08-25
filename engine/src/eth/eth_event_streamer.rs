use anyhow::Result;
use futures::{stream, Stream, StreamExt};
use std::fmt::Debug;
use web3::{
    ethabi::RawLog,
    transports::WebSocket,
    types::{BlockNumber, FilterBuilder, H160, H256},
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
) -> Result<impl Stream<Item = Result<Event>>> {
    // The `fromBlock` parameter doesn't seem to work reliably with subscription streams, so
    // request past block via http and prepend them to the stream manually.
    let past_logs = web3
        .eth()
        .logs(
            FilterBuilder::default()
                .from_block(BlockNumber::Number(from_block.into()))
                .address(vec![deployed_address])
                .build(),
        )
        .await?;

    let future_logs = web3
        .eth_subscribe()
        .subscribe_logs(
            FilterBuilder::default()
                .from_block(BlockNumber::Pending)
                .address(vec![deployed_address])
                .build(),
        )
        .await?;

    let logger = logger.clone();
    Ok(stream::iter(past_logs)
        .map(|log| Ok(log))
        .chain(future_logs)
        .map(move |result_unparsed_log| -> Result<Event, anyhow::Error> {
            let result_event = result_unparsed_log
                .map_err(|error| anyhow::Error::new(error))
                .and_then(|log| {
                    decode_log(
                        /*signature*/
                        *log.topics.first().ok_or_else(|| {
                            anyhow::Error::msg("Could not get signature from ETH log")
                        })?,
                        /*tx hash*/
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
