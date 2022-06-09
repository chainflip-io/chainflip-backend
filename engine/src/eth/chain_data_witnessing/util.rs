use std::time::Duration;

use futures::{future, stream, Stream, StreamExt};
use sp_runtime::traits::Bounded;
use tokio::sync::watch;

/// Returns a stream that yields `()` at regular intervals.
pub fn tick_stream(tick_interval: Duration) -> impl Stream<Item = ()> {
    stream::unfold(tokio::time::interval(tick_interval), |mut interval| async {
        interval.tick().await;
        Some(((), interval))
    })
}

/// Bounds a stream of ordered items such that the first item is greater than `start`. Returns another stream and a
/// Sender that can be used to push an `end` item. The stream will continue yielding so long as:
/// 1. No `end` item has been received.
/// 2. The next item to be yielded is less than or equal to the `end` item.
pub fn bounded<'a, Item: 'a + Ord + Send + Sync>(
    start: Item,
    stream: impl Stream<Item = Item> + Send + 'a,
) -> (impl Stream<Item = Item>, watch::Sender<Option<Item>>) {
    let (sender, receiver) = watch::channel::<Option<Item>>(None);
    (
        stream
            .skip_while(move |item| future::ready(*item < start))
            .take_while(move |item| {
                future::ready(
                    receiver
                        .borrow()
                        .as_ref()
                        .map(|end| item <= end)
                        .unwrap_or(true),
                )
            }),
        sender,
    )
}

/// Takes a stream of items and ensures that they are strictly increasing according to some ordering, meaning that
/// any item yielded should be greater than the previous one.
pub fn strictly_increasing<Item: Bounded + Ord + Clone + Send + Sync>(
    stream: impl Stream<Item = Item> + Send,
) -> impl Stream<Item = Item> {
    stream
        .scan(Bounded::min_value(), |max, next| {
            future::ready(Some(if next > *max {
                *max = next.clone();
                Some(next)
            } else {
                None
            }))
        })
        .filter_map(future::ready)
}

#[cfg(test)]
mod test {

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

    #[tokio::test]
    async fn test_strictly_increasing() {
        assert_eq!(
            strictly_increasing(stream::iter(vec![2, 2, 1, 3, 4, 2, 5]))
                .collect::<Vec<u64>>()
                .await,
            (2..=5).collect::<Vec<_>>()
        );
        assert_eq!(
            strictly_increasing(stream::iter(vec![2, 2, 1, 0]))
                .collect::<Vec<u64>>()
                .await,
            vec![2]
        );
    }
}
