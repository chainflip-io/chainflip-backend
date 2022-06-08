pub mod http;
pub mod ws;

use std::time::Duration;

use super::rpc::{EthRpcApi, EthWsRpcApi};

use cf_chains::{eth::TrackedData, Ethereum};
use futures::{
    future,
    stream::{self, select_all, BoxStream},
    Stream, StreamExt,
};
use tokio::sync::watch;

use sp_core::U256;
use web3::types::{BlockNumber, U64};

/// Returns a stream that yields `()` at regular intervals.
fn tick_stream(tick_interval: Duration) -> impl Stream<Item = ()> {
    stream::unfold(tokio::time::interval(tick_interval), |mut interval| async {
        interval.tick().await;
        Some(((), interval))
    })
}

/// Bounds a stream of ordered items such that the first item is greater than `start`. Returns another stream and a
/// Sender that can be used to push an `end` item. The stream will continue yielding so long as:
/// 1. No `end` item has been received.
/// 2. The next item to be yielded is less than or equal to the `end` item.
fn bounded<'a, Item: 'a + Ord + Send + Sync>(
    start: Item,
    stream: impl Stream<Item = Item> + Send + 'a,
) -> (BoxStream<'a, Item>, watch::Sender<Option<Item>>) {
    let (sender, receiver) = watch::channel::<Option<Item>>(None);
    (
        Box::pin(
            stream
                .skip_while(move |item| future::ready(*item < start))
                .take_while(move |item| {
                    future::ready(
                        receiver
                            .borrow()
                            .as_ref()
                            .map(|end| item <= &end)
                            .unwrap_or(true),
                    )
                }),
        ),
        sender,
    )
}

/// Returns a stream that yields the latest known fee data.
///
/// Collects the data from the websocket and http protocols in parallel and yields whichever has the
/// highest block number.
pub async fn chain_data_witnesser<'a, EthWsRpcClient, EthHttpRpcClient>(
    from_block: u64,
    http_polling_interval: Duration,
    eth_ws_rpc: &'a EthWsRpcClient,
    eth_http_rpc: &'a EthHttpRpcClient,
    logger: &slog::Logger,
) -> anyhow::Result<(
    BoxStream<'a, anyhow::Result<TrackedData<Ethereum>>>,
    watch::Sender<Option<u64>>,
)>
where
    EthWsRpcClient: EthWsRpcApi + EthRpcApi + Send + Sync,
    EthHttpRpcClient: EthRpcApi + Send + Sync,
{
    let (combined_stream, end_block_sender) = bounded(
        from_block,
        select_all([
            ws::latest_block_numbers(eth_ws_rpc, logger).await?,
            http::latest_block_numbers(eth_http_rpc, http_polling_interval, logger),
        ])
        .scan(0, |highest, block_number| {
            future::ready(Some(if block_number > *highest {
                *highest = block_number;
                Some(block_number)
            } else {
                None
            }))
        })
        .filter_map(future::ready),
    );

    Ok((
        Box::pin(combined_stream.then(move |block_number| async move {
            let fee_history = future::select_ok([
                eth_ws_rpc.fee_history(
                    U256::one(),
                    BlockNumber::Number(U64::from(block_number)),
                    Some(vec![0.5]),
                ),
                eth_http_rpc.fee_history(
                    U256::one(),
                    BlockNumber::Number(U64::from(block_number)),
                    Some(vec![0.5]),
                ),
            ])
            .await?
            .0;

            Ok(TrackedData::<Ethereum> {
                block_height: block_number,
                base_fee: fee_history
                    .base_fee_per_gas
                    .first()
                    .expect("Requested, so should be present.")
                    .as_u128(),
                priority_fee: fee_history
                    .reward
                    .expect("Requested, so should be present.")
                    .first()
                    .expect("Requested, so should be present.")
                    .first()
                    .expect("Requested, so should be present.")
                    .as_u128(),
            })
        })),
        end_block_sender,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tick_stream() {
        const INTERVAL_MS: u64 = 10;
        const REPETITIONS: usize = 10;
        // 10 ticks is equivalent to 9 intervals.
        const EXPECTED_DURATION_MS: u64 = INTERVAL_MS * (REPETITIONS - 1) as u64;

        let start = std::time::Instant::now();
        let result = tick_stream(Duration::from_millis(INTERVAL_MS))
            .scan(0, |count, _| {
                *count += 1;
                future::ready(Some(*count))
            })
            .take(REPETITIONS)
            .collect::<Vec<_>>()
            .await;
        let end = std::time::Instant::now();
        assert!(
            end - start >= Duration::from_millis(EXPECTED_DURATION_MS),
            "Expected {:?} >= {:?} ms.",
            (end - start).as_millis(),
            EXPECTED_DURATION_MS,
        );
        assert_eq!(result, (1..=REPETITIONS).collect::<Vec<_>>());
    }

    #[tokio::test]
    async fn test_bounded() {
        const START: u64 = 10;
        const END: u64 = 20;

        let (stream, end_block_sender) = bounded(START, stream::iter(0..));

        let handle = tokio::spawn(async move { stream.collect::<Vec<_>>().await });
        end_block_sender.send(Some(END)).unwrap();

        let result = handle.await.unwrap();

        assert_eq!(result, (START..=END).collect::<Vec<_>>());
    }
}
