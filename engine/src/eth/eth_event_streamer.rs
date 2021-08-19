use crate::eth::EventProducerError;

use futures::{StreamExt, stream};
use tokio::sync::mpsc::UnboundedSender;
use web3::{transports::WebSocket, Web3, ethabi::RawLog, types::{BlockNumber, FilterBuilder, H160, H256}};
use anyhow::Result;

pub async fn start<Event, Parser>(
    web3 : Web3<WebSocket>,
    deployed_address : H160,
    from_block: u64,
    parser : Parser,
    sink : UnboundedSender<Event>,
    logger: slog::Logger,
) -> Result<()> where
    Parser : Fn(H256, H256, RawLog) -> Result<Event>
{
    slog::info!(
        logger,
        "Start running eth event stream from block: {:?}",
        from_block
    );

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

    let mut event_stream = stream::iter(past_logs)
        .map(|log| Ok(log))
        .chain(future_logs).map(|log_result| {
            let log = log_result?;

            let sig = log
                .topics
                .first()
                .ok_or_else(|| EventProducerError::EmptyTopics)?
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

            parser(sig, tx_hash, raw_log)
        });

        while let Some(event) = event_stream.next().await {
            assert!(sink.send(event.unwrap()).is_ok());
        }

    Ok(())
}

#[cfg(test)]
mod tests {

    use crate::{
        eth::{new_web3_client, stake_manager::stake_manager::StakeManager},
        logging,
        settings,
    };

    use super::*;

    #[tokio::test]
    #[ignore = "Depends on a running ganache instance, runs forever, useful for manually testing / observing incoming events"]
    async fn subscribe_to_stake_manager_events() {
        let logger = logging::test_utils::create_test_logger();

        let settings = settings::test_utils::new_test_settings().unwrap();

        let stake_manager = StakeManager::new(&settings).unwrap();

        start(
            new_web3_client(&settings, &logger).await.unwrap(),
            stake_manager.deployed_address,
            0,
            stake_manager.parser_closure().unwrap(),
            tokio::sync::mpsc::unbounded_channel().0,
            logger
        ).await.unwrap();
    }
}
