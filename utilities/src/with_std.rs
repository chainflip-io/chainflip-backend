use core::{fmt, time::Duration};
use futures::{stream, Stream};
#[doc(hidden)]
pub use lazy_format::lazy_format as internal_lazy_format;
use std::path::PathBuf;

pub fn clean_hex_address<const LEN: usize>(address_str: &str) -> Result<[u8; LEN], &'static str> {
	let address_hex_str = match address_str.strip_prefix("0x") {
		Some(address_stripped) => address_stripped,
		None => address_str,
	};

	let address: [u8; LEN] = hex::decode(address_hex_str)
		.map_err(|_| "Invalid hex")?
		.try_into()
		.map_err(|_| "Invalid address length")?;

	Ok(address)
}

pub fn clean_eth_address(dirty_eth_address: &str) -> Result<[u8; 20], &'static str> {
	clean_hex_address(dirty_eth_address)
}

pub fn clean_dot_address(dirty_dot_address: &str) -> Result<[u8; 32], &'static str> {
	clean_hex_address(dirty_dot_address)
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

/// Makes a tick that outputs every duration and if ticks are "missed" (as tick() wasn't called for
/// some time) it will immediately output a single tick on the next call to tick() and resume
/// ticking every duration.
///
/// The supplied duration should be >> 5ms due to the underlying implementation of
/// [Interval::poll_tick].
pub fn make_periodic_tick(duration: Duration, yield_immediately: bool) -> tokio::time::Interval {
	let mut interval = tokio::time::interval_at(
		tokio::time::Instant::now() + if yield_immediately { Duration::ZERO } else { duration },
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
			assert!(tokio::time::timeout(Duration::from_secs_f32(PERIOD * 0.8), tick.tick())
				.await
				.is_err());
			tick.tick().await;
		}
	}
}

/// Returns a stream that yields `()` at regular intervals. Uses tokio's [MissedTickBehavior::Delay]
/// tick strategy, meaning ticks will always be at least `interval` duration apart.
///
/// Suitable for polling.
///
/// Note that in order for this to work as expected, due to the underlying implementation of
/// [Interval::poll_tick], the polling interval should be >> 5ms.
pub fn periodic_tick_stream(tick_interval: Duration) -> impl Stream<Item = ()> {
	stream::unfold(make_periodic_tick(tick_interval, true), |mut interval| async {
		interval.tick().await;
		Some(((), interval))
	})
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

pub fn repository_link() -> Option<impl core::fmt::Display> {
	core::option_env!("COMMIT_HASH").map(|commit_hash| {
		lazy_format::lazy_format!(
			"https://github.com/chainflip-io/chainflip-backend/tree/{commit_hash}"
		)
	})
}

#[macro_export]
macro_rules! here {
	() => {
		utilities::internal_lazy_format!(
			if let Some(repository_link) = utilities::repository_link() => (
				"{}/{}#L{}#C{}",
				repository_link,
				file!(),
				line!(),
				column!()
			)
			// Add a special cool method for adding line numbers
			// Ripped from: https://github.com/dtolnay/anyhow/issues/22
			else => ("{}", concat!(file!(), ":", line!(), ":", column!()))
		)
	};
}

#[macro_export]
macro_rules! context {
	($e:expr) => {{
		// Using function ensures the expression's temporary's lifetimes last until after context!()
		// call
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

#[macro_export]
macro_rules! print_starting {
	() => {
		println!(
			"Starting {} v{} ({})",
			env!("CARGO_PKG_NAME"),
			env!("CARGO_PKG_VERSION"),
			utilities::internal_lazy_format!(if let Some(repository_link) = utilities::repository_link() => ("CI Build: \"{}\"", repository_link) else => ("Non-CI Build"))
		);
		println!(
			"
			 ██████╗██╗  ██╗ █████╗ ██╗███╗   ██╗███████╗██╗     ██╗██████╗
			██╔════╝██║  ██║██╔══██╗██║████╗  ██║██╔════╝██║     ██║██╔══██╗
			██║     ███████║███████║██║██╔██╗ ██║█████╗  ██║     ██║██████╔╝
			██║     ██╔══██║██╔══██║██║██║╚██╗██║██╔══╝  ██║     ██║██╔═══╝
			╚██████╗██║  ██║██║  ██║██║██║ ╚████║██║     ███████╗██║██║
			 ╚═════╝╚═╝  ╚═╝╚═╝  ╚═╝╚═╝╚═╝  ╚═══╝╚═╝     ╚══════╝╚═╝╚═╝
			"
		);
	}
}

// We use PathBuf because the value must be Sized, Path is not Sized
pub fn deser_path<'de, D>(deserializer: D) -> std::result::Result<PathBuf, D::Error>
where
	D: serde::Deserializer<'de>,
{
	struct PathVisitor;

	impl<'de> serde::de::Visitor<'de> for PathVisitor {
		type Value = PathBuf;

		fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
			formatter.write_str("A string containing a path")
		}

		fn visit_str<E>(self, v: &str) -> std::result::Result<Self::Value, E>
		where
			E: serde::de::Error,
		{
			Ok(PathBuf::from(v))
		}
	}

	// use our visitor to deserialize a `PathBuf`
	deserializer.deserialize_any(PathVisitor)
}

pub fn read_clean_and_decode_hex_str_file<V, T: FnOnce(&str) -> Result<V, anyhow::Error>>(
	file: &std::path::Path,
	context: &str,
	t: T,
) -> Result<V, anyhow::Error> {
	use anyhow::Context;

	std::fs::read_to_string(file)
		.map_err(anyhow::Error::new)
		.with_context(|| format!("Failed to read {} file at {}", context, file.display()))
		.and_then(|string| {
			let mut str = string.as_str();
			str = str.trim();
			str = str.trim_matches(['"', '\''].as_ref());
			if let Some(stripped_str) = str.strip_prefix("0x") {
				str = stripped_str;
			}
			// Note if str is valid hex or not is determined by t()
			t(str)
		})
		.with_context(|| format!("Failed to decode {} file at {}", context, file.display()))
}

#[cfg(test)]
mod tests_read_clean_and_decode_hex_str_file {
	use crate::{assert_ok, testing::with_file};

	use super::read_clean_and_decode_hex_str_file;

	#[test]
	fn load_hex_file() {
		with_file(b"   \"\'\'\"0xhex\"\'  ", |file_path| {
			assert_eq!(
				assert_ok!(read_clean_and_decode_hex_str_file(file_path, "TEST", |str| Ok(
					str.to_string()
				))),
				"hex".to_string()
			);
		});
	}

	#[test]
	fn load_invalid_hex_file() {
		with_file(b"   h\" \'ex  ", |file_path| {
			assert_eq!(
				assert_ok!(read_clean_and_decode_hex_str_file(file_path, "TEST", |str| Ok(
					str.to_string()
				))),
				"h\" \'ex".to_string()
			);
		});
	}
}

pub fn format_iterator<'a, It: 'a + IntoIterator>(it: It) -> itertools::Format<'a, It::IntoIter>
where
	It::Item: fmt::Display,
{
	use itertools::Itertools;
	it.into_iter().format(", ")
}

pub fn all_same<Item: PartialEq, It: IntoIterator<Item = Item>>(it: It) -> Option<Item> {
	let mut it = it.into_iter();
	let option_item = it.next();
	match option_item {
		Some(item) =>
			if it.all(|other_items| other_items == item) {
				Some(item)
			} else {
				None
			},
		None => panic!(),
	}
}

pub fn split_at<C: FromIterator<It::Item>, It: IntoIterator>(it: It, index: usize) -> (C, C)
where
	It::IntoIter: ExactSizeIterator,
{
	struct IteratorRef<'a, T, It: Iterator<Item = T>> {
		it: &'a mut It,
	}
	impl<'a, T, It: Iterator<Item = T>> Iterator for IteratorRef<'a, T, It> {
		type Item = T;

		fn next(&mut self) -> Option<Self::Item> {
			self.it.next()
		}
	}

	let mut it = it.into_iter();
	assert!(index < it.len());
	let wrapped_it = IteratorRef { it: &mut it };
	(wrapped_it.take(index).collect(), it.collect())
}

#[test]
fn test_split_at() {
	let (left, right) = split_at::<Vec<_>, _>(vec![4, 5, 6, 3, 4, 5], 3);

	assert_eq!(&left[..], &[4, 5, 6]);
	assert_eq!(&right[..], &[3, 4, 5]);
}
