use core::{fmt::Display, time::Duration};
use futures::{stream, Stream};

#[macro_export]
macro_rules! assert_panics {
    ($expression:expr) => {
        match ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(|| $expression)) {
            Ok(_result) => panic!("expression didn't panic '{}'", stringify!($expression),),
            Err(panic) => panic,
        }
    };
}

#[macro_export]
macro_rules! assert_future_panics {
    ($future:expr) => {
        use futures::future::FutureExt;
        match ::std::panic::AssertUnwindSafe($future).catch_unwind().await {
            Ok(_result) => panic!("future didn't panic '{}'", stringify!($future),),
            Err(panic) => panic,
        }
    };
}

/// Makes a tick that outputs every duration and if ticks are "missed" (as tick() wasn't called for some time)
/// it will immediately output a single tick on the next call to tick() and resume ticking every duration.
///
/// The supplied duration should be >> 5ms due to the underlying implementation of [Intervall::poll_tick].
pub fn make_periodic_tick(duration: Duration, yield_immediately: bool) -> tokio::time::Interval {
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
    use crate::{assert_err, assert_ok};

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
        assert_err!(tokio::time::timeout(Duration::from_secs_f32(PERIOD * 0.9), tick.tick()).await);
        assert_ok!(tokio::time::timeout(Duration::from_secs_f32(PERIOD * 0.1), tick.tick()).await);

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
