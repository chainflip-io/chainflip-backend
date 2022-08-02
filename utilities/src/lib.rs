#![cfg_attr(not(feature = "std"), no_std)]

pub type Port = u16;

/// Simply unwraps the value. Advantage of this is to make it clear in tests
/// what we are testing
#[macro_export]
macro_rules! assert_ok {
    ($result:expr) => {
        $result.unwrap()
    };
}

#[macro_export]
macro_rules! assert_err {
    ($result:expr) => {
        $result.unwrap_err()
    };
}

mod test_asserts {

    #[test]
    fn test_assert_ok_unwrap_ok() {
        fn works() -> Result<i32, i32> {
            Ok(1)
        }
        let result = assert_ok!(works());
        assert_eq!(result, 1);
    }

    #[test]
    #[should_panic]
    fn test_assert_ok_err() {
        fn works() -> Result<i32, i32> {
            Err(0)
        }
        assert_ok!(works());
    }
}

/// Note that the resulting `threshold` is the maximum number
/// of parties *not* enough to generate a signature,
/// i.e. at least `t+1` parties are required.
/// This follows the notation in the multisig library that
/// we are using and in the corresponding literature.
///
/// For the *success* threshold, use [success_threshold_from_share_count].
pub fn threshold_from_share_count(share_count: u32) -> u32 {
    if 0 == share_count {
        0
    } else {
        (share_count.checked_mul(2).unwrap() - 1) / 3
    }
}

/// Returns the number of parties required for a threshold signature
/// ceremony to *succeed*.
pub fn success_threshold_from_share_count(share_count: u32) -> u32 {
    threshold_from_share_count(share_count)
        .checked_add(1)
        .unwrap()
}

/// Returns the number of bad parties required for a threshold signature
/// ceremony to *fail*.
pub fn failure_threshold_from_share_count(share_count: u32) -> u32 {
    share_count - threshold_from_share_count(share_count)
}

#[test]
fn check_threshold_calculation() {
    assert_eq!(threshold_from_share_count(150), 99);
    assert_eq!(threshold_from_share_count(100), 66);
    assert_eq!(threshold_from_share_count(90), 59);
    assert_eq!(threshold_from_share_count(3), 1);
    assert_eq!(threshold_from_share_count(4), 2);

    assert_eq!(success_threshold_from_share_count(150), 100);
    assert_eq!(success_threshold_from_share_count(100), 67);
    assert_eq!(success_threshold_from_share_count(90), 60);
    assert_eq!(success_threshold_from_share_count(3), 2);
    assert_eq!(success_threshold_from_share_count(4), 3);

    assert_eq!(failure_threshold_from_share_count(150), 51);
    assert_eq!(failure_threshold_from_share_count(100), 34);
    assert_eq!(failure_threshold_from_share_count(90), 31);
    assert_eq!(failure_threshold_from_share_count(3), 2);
    assert_eq!(failure_threshold_from_share_count(4), 2);
}

pub fn clean_eth_address(dirty_eth_address: &str) -> Result<[u8; 20], &str> {
    let eth_address_hex_str = match dirty_eth_address.strip_prefix("0x") {
        Some(eth_address_stripped) => eth_address_stripped,
        None => dirty_eth_address,
    };

    let eth_address: [u8; 20] = hex::decode(eth_address_hex_str)
        .map_err(|_| "Invalid hex")?
        .try_into()
        .map_err(|_| "Could not create a [u8; 20]")?;

    Ok(eth_address)
}

#[test]
fn cleans_eth_address() {
    // fail too short
    let input = "0x323232";
    assert!(clean_eth_address(input).is_err());

    // fail invalid chars
    let input = "0xZ29aB9EbDb421CE48b70flippya6e9a3DBD609C5";
    assert!(clean_eth_address(input).is_err());

    // success with 0x
    let input = "0xB29aB9EbDb421CE48b70699758a6e9a3DBD609C5";
    assert!(clean_eth_address(input).is_ok());

    // success without 0x
    let input = "B29aB9EbDb421CE48b70699758a6e9a3DBD609C5";
    assert!(clean_eth_address(input).is_ok());
}

#[cfg(feature = "std")]
mod with_std {
    use core::{fmt::Display, time::Duration};
    use futures::{stream, Stream};

    /// Makes a tick that outputs every duration and if ticks are "missed" (as tick() wasn't called for some time)
    /// it will immediately output a single tick on the next call to tick() and resume ticking every duration.
    ///
    /// The supplied duration should be >> 5ms due to the underlying implementation of [Intervall::poll_tick].
    pub fn make_periodic_tick(
        duration: Duration,
        yield_immediately: bool,
    ) -> tokio::time::Interval {
        let mut interval = tokio::time::interval_at(
            tokio::time::Instant::now()
                + if yield_immediately {
                    Duration::ZERO
                } else {
                    duration
                },
            duration,
        );
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        interval
    }

    #[cfg(test)]
    mod tests_make_periodic_tick {
        use super::*;

        #[tokio::test]
        async fn skips_ticks_test() {
            const PERIOD: f32 = 0.25;

            let mut tick = make_periodic_tick(Duration::from_secs_f32(PERIOD), false);

            // Skip two ticks
            tokio::time::sleep(Duration::from_secs_f32(PERIOD * 2.5)).await;

            // Next tick outputs immediately
            assert_ok!(tokio::time::timeout(Duration::from_secs_f32(0.01), tick.tick()).await);

            // We skip ticks instead of bursting ticks.
            assert_err!(
                tokio::time::timeout(Duration::from_secs_f32(PERIOD * 0.9), tick.tick()).await
            );
            assert_ok!(
                tokio::time::timeout(Duration::from_secs_f32(PERIOD * 0.1), tick.tick()).await
            );

            // Ticks continue to be in sync with duration.
            assert_err!(
                tokio::time::timeout(Duration::from_secs_f32(PERIOD * 0.95), tick.tick()).await
            );
            assert_ok!(tokio::time::timeout(Duration::from_secs_f32(PERIOD), tick.tick()).await);
        }

        #[tokio::test]
        async fn period_test() {
            const PERIOD: f32 = 0.25;

            let mut tick = make_periodic_tick(Duration::from_secs_f32(PERIOD), false);

            for _i in 0..4 {
                assert!(
                    tokio::time::timeout(Duration::from_secs_f32(PERIOD * 0.8), tick.tick())
                        .await
                        .is_err()
                );
                tick.tick().await;
            }
        }
    }

    /// Returns a stream that yields `()` at regular intervals. Uses tokio's [MissedTickBehavior::Delay] tick strategy,
    /// meaning ticks will always be at least `interval` duration apart.
    ///
    /// Suitable for polling.
    ///
    /// Note that in order for this to work as expected, due to the underlying implementation of [Interval::poll_tick], the
    /// polling interval should be >> 5ms.
    pub fn periodic_tick_stream(tick_interval: Duration) -> impl Stream<Item = ()> {
        stream::unfold(
            make_periodic_tick(tick_interval, true),
            |mut interval| async {
                interval.tick().await;
                Some(((), interval))
            },
        )
    }

    #[cfg(test)]
    mod tests_periodic_tick_stream {
        use core::future;

        use futures::StreamExt;

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
    }

    // Needed due to the jsonrpc maintainer's not definitely unquestionable decision to impl their error types without the Sync trait
    pub fn rpc_error_into_anyhow_error(error: jsonrpc_core_client::RpcError) -> anyhow::Error {
        anyhow::Error::msg(error.to_string())
    }

    pub trait JsonResultExt {
        type T;

        fn map_to_json_error(self) -> jsonrpc_core::Result<Self::T>;
    }

    pub fn new_json_error<E: Display>(error: E) -> jsonrpc_core::Error {
        jsonrpc_core::Error {
            code: jsonrpc_core::ErrorCode::ServerError(0),
            message: error.to_string(),
            data: None,
        }
    }

    impl<T, E: Display> JsonResultExt for std::result::Result<T, E> {
        type T = T;

        fn map_to_json_error(self) -> jsonrpc_core::Result<Self::T> {
            self.map_err(new_json_error)
        }
    }

    pub mod mockall_utilities {
        use mockall::Predicate;
        use predicates::reflection::PredicateReflection;

        // Allows equality predicate between differing types
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct EqPredicate<T> {
            constant: T,
        }
        impl<T: std::fmt::Debug, P: ?Sized> Predicate<P> for EqPredicate<T>
        where
            P: std::fmt::Debug + PartialEq<T>,
        {
            fn eval(&self, variable: &P) -> bool {
                variable.eq(&self.constant)
            }
        }
        impl<T: std::fmt::Debug> PredicateReflection for EqPredicate<T> {}
        impl<T: std::fmt::Debug> std::fmt::Display for EqPredicate<T> {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "var {:?}", self.constant)
            }
        }
        pub fn eq<T>(constant: T) -> EqPredicate<T> {
            EqPredicate { constant }
        }
    }

    #[macro_export]
    macro_rules! here {
        () => {
            lazy_format::lazy_format!(
                // CIRCLE_SHA1 is CircleCI's environment variable exposing the git commit hash
                if let Some(commit_hash) = core::option_env!("CIRCLE_SHA1") => (
                    "https://github.com/chainflip-io/chainflip-backend/tree/{commit_hash}/{}#L{}#C{}",
                    file!(),
                    line!(),
                    column!()
                )
                // Add a special cool method for adding line numbers
                // Ripped from: https://github.com/dtolnay/anyhow/issues/22
                else => ("{}", concat!(file!(), " line ", line!(), " column ", column!()))
            )
        };
    }

    #[macro_export]
    macro_rules! context {
        ($e:expr) => {{
            // Using function ensures the expression's temporary's lifetimes last until after context!() call
            #[inline(always)]
            fn get_expr_type<V, E, T: anyhow::Context<V, E>, Here: core::fmt::Display>(
                t: T,
                here: Here,
            ) -> anyhow::Result<V> {
                t.with_context(|| {
                    format!(
                        "Error: '{}' with type '{}' failed at {}",
                        stringify!($e),
                        std::any::type_name::<T>(),
                        here
                    )
                })
            }

            get_expr_type($e, utilities::here!())
        }};
    }
}

#[cfg(feature = "std")]
pub use with_std::*;
