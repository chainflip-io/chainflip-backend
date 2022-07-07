use std::time::Duration;

use futures::{future, stream, Stream, StreamExt};
use sp_runtime::traits::Bounded;
use tokio::sync::oneshot;

/// Returns a stream that yields `()` at regular intervals. Uses tokio's [MissedTickBehavior::Delay] tick strategy,
/// meaning ticks will always be at least `interval` duration apart.
///
/// The first tick yields immediately. Suitable for polling.
///
/// Note that in order for this to work as expected, due to the underlying implementation of [Interval::poll_tick], the
/// polling interval should be >> 5ms.
pub fn periodic_tick_stream(tick_interval: Duration) -> impl Stream<Item = ()> {
    let mut interval = tokio::time::interval(tick_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    stream::unfold(interval, |mut interval| async {
        interval.tick().await;
        Some(((), interval))
    })
}

/// Bounds a stream of ordered items such that the first item is greater than `start`. Returns another stream and a
/// Sender that can be used to push an `end` item. The stream will continue yielding so long as:
/// 1. No `end` item has been received.
/// 2. The next item to be yielded is less than or equal to the `end` item.
///
/// Start is inclusive, End is exclusive.
///
/// The end_receiver should be freshly created from using [watch::Sender::subscribe].
///
/// It's possible to send multiple `end` items over the channel. In this case, the last one overrides any previous ones.
pub fn bounded<'a, Item: 'a + Ord + Send + Sync>(
    start: Item,
    mut end_receiver: oneshot::Receiver<Item>,
    stream: impl Stream<Item = Item> + Send + 'a,
) -> impl Stream<Item = Item> + 'a {
    let mut option_end_bound = None;
    stream
        .skip_while(move |item| future::ready(*item < start))
        .take_while(move |item| {
            future::ready({
                if let Some(end_bound) = &option_end_bound {
                    Some(end_bound)
                } else {
                    match end_receiver.try_recv() {
                        Ok(end_bound) => Some(&*option_end_bound.insert(end_bound)),
                        Err(_) => None,
                    }
                }
                .map_or(true, |end_bound| item < end_bound)
            })
        })
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
        let result = periodic_tick_stream(Duration::from_millis(INTERVAL_MS))
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

        let (end_sender, end_receiver) = oneshot::channel();

        let handle = tokio::spawn(async move {
            bounded(START, end_receiver, stream::iter(0..))
                .collect::<Vec<_>>()
                .await
        });
        end_sender.send(END).unwrap();

        let result = handle.await.unwrap();

        assert_eq!(result, (START..END).collect::<Vec<_>>());
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
