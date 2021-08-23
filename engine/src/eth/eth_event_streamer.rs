use anyhow::Result;
use futures::{stream, Stream, StreamExt};
use web3::{
    ethabi::RawLog,
    transports::WebSocket,
    types::{BlockNumber, FilterBuilder, H160, H256},
    Web3,
};

/// Creates a stream that outputs the (signature, transaction hash, raw log) of events from a contract.
pub async fn new_eth_event_stream(
    web3: Web3<WebSocket>,
    deployed_address: H160,
    from_block: u64,
    logger: slog::Logger,
) -> Result<impl Stream<Item = Result<(H256, H256, RawLog)>>> {
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

    Ok(stream::iter(past_logs)
        .map(|log| Ok(log))
        .chain(future_logs)
        .map(
            move |result_unparsed_log| -> Result<(H256, H256, RawLog), anyhow::Error> {
                let result_extracted_log_details = result_unparsed_log
                    .map_err(|error| anyhow::Error::new(error))
                    .and_then(|log| {
                        Ok((
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
                        ))
                    });

                slog::debug!(
                    logger,
                    "Received ETH log: {:?}",
                    result_extracted_log_details
                );

                result_extracted_log_details
            },
        ))
}

#[cfg(test)]
mod tests {

    use std::str::FromStr;

    use crate::{eth::new_web3_client, logging, settings};

    use super::*;

    #[tokio::test]
    #[ignore = "Depends on a running ganache instance, runs forever, useful for manually testing / observing incoming events"]
    async fn subscribe_to_stake_manager_events() {
        let logger = logging::test_utils::create_test_logger();

        let settings = settings::test_utils::new_test_settings().unwrap();

        new_eth_event_stream(
            new_web3_client(&settings, &logger).await.unwrap(),
            H160::from_str(&settings.eth.key_manager_eth_address).unwrap(),
            0,
            logger,
        )
        .await
        .unwrap()
        .collect::<Vec<_>>()
        .await;
    }
}
