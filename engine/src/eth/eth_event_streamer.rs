use crate::logging::COMPONENT_KEY;

use super::{EventSink, EventSource};
use futures::{future::join_all, stream, StreamExt};
use slog::o;
use web3::{
    types::BlockNumber,
    ethabi::RawLog,
    Web3,
    Transport,
    DuplexTransport
};

use anyhow::Result;

pub async fn start<Event, Parser, EventSink, T>(
    web3 : &Web3<T>,
    deployed_address : H160,
    from_block: u64,
    parser : Parser,
    sink : EventSink,
    logger: &slog::Logger,
)  -> impl futures::Future where
    Parser : Fn(H256, H256, RawLog) -> Result<Event>,
    EventSink : Sink<Event>,
    T : DuplexTransport
{
    let logger = logger.new(o!(COMPONENT_KEY => "EthEventStreamer"));

    slog::info!(
        logger,
        "Start running eth event stream from block: {:?}",
        from_block
    );

    // The `fromBlock` parameter doesn't seem to work reliably with subscription streams, so
    // request past block via http and prepend them to the stream manually.
    let past_logs = web3.eth()
        .logs(FilterBuilder::default()
            .from_block(BlockNumber::Number(from_block))
            .address(vec![deployed_address])
        )
        .await?;

    let future_logs = self
        .web3
        .eth_subscribe()
        .subscribe_logs(FilterBuilder::default()
            .from_block(BlockNumber::Pending)
            .address(vec![deployed_address])
        )
        .await?;

    stream::iter(past_logs)
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
                ))?
                .to_fixed_bytes();

            let raw_log = ethabi::RawLog {
                topics: log.topics,
                data: log.data.0,
            };

            slog::debug!(
                self.logger,
                "Parsing event from block {:?} with signature: {:?}",
                log.block_number.unwrap_or_default(),
                sig
            );

            parser(sig, tx_hash, raw_log)
        }).for_each(|event| {
            sink.send(event.unwrap()).unwrap();
        })
}

#[cfg(test)]
mod tests {

    use crate::{
        eth::{new_web3_client, stake_manager::stake_manager::StakeManager},
        logging,
        mq::nats_client::NatsMQClient,
        settings,
    };

    use super::*;

    const CONTRACT_ADDRESS: &'static str = "0xEAd5De9C41543E4bAbB09f9fE4f79153c036044f";

    #[tokio::test]
    #[ignore = "Depends on a running ganache instance, runs forever, useful for manually testing / observing incoming events"]
    async fn subscribe_to_stake_manager_events() {
        let logger = logging::test_utils::create_test_logger();

        let settings = settings::test_utils::new_test_settings().unwrap();

        let mq_client = NatsMQClient::new(&settings.message_queue).await.unwrap();

        start(
            &new_web3_client(&settings, &logger).await.unwrap(),
            CONTRACT_ADDRESS,
            0,
            ,
            ,
            &logger
        ).await;
    }
}
