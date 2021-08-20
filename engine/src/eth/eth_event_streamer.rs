use crate::eth::EventParseError;

use futures::{Stream, StreamExt, stream};
use web3::{transports::WebSocket, Web3, ethabi::RawLog, types::{BlockNumber, FilterBuilder, H160, H256}};
use anyhow::Result;

pub async fn new_eth_event_stream(
    web3 : Web3<WebSocket>,
    deployed_address : H160,
    from_block: u64,
    logger: slog::Logger,
) -> Result<impl Stream<Item = Result<(H256, H256, RawLog)>>> {
    // The `fromBlock` parameter doesn't seem to work reliably with subscription streams, so
    // request past block via http and prepend them to the stream manually.
    let past_logs = web3.eth()
        .logs(FilterBuilder::default()
            .from_block(BlockNumber::Number(from_block.into()))
            .address(vec![deployed_address])
            .build()
        )
        .await?;

    let future_logs = web3
        .eth_subscribe()
        .subscribe_logs(FilterBuilder::default()
            .from_block(BlockNumber::Pending)
            .address(vec![deployed_address])
            .build()
        )
        .await?;

    Ok(stream::iter(past_logs)
        .map(|log| Ok(log))
        .chain(future_logs).map(move |log_result| -> Result<(H256, H256, RawLog), anyhow::Error> {
            let log = log_result?;

            let sig = log
                .topics
                .first()
                .ok_or_else(|| EventParseError::EmptyTopics)?
                .clone();

            let tx_hash = log
                .transaction_hash
                .ok_or(anyhow::Error::msg(
                    "Could not get transaction hash from ETH log",
                ))?;

            let raw_log = RawLog {
                topics: log.topics,
                data: log.data.0,
            };

            slog::debug!(
                logger,
                "Parsing event from block {:?} with signature: {:?}",
                log.block_number.unwrap_or_default(),
                sig
            );

            Ok((sig, tx_hash, raw_log))
        })
    )
}

#[cfg(test)]
mod tests {

    use std::str::FromStr;

    use crate::{
        eth::new_web3_client,
        logging,
        settings,
    };

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
            logger
        ).await.unwrap().collect::<Vec<_>>().await;
    }
}
