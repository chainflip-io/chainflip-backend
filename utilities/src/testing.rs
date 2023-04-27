use std::{
	io::Write,
	path::{Path, PathBuf},
};

use core::time::Duration;
use futures::{Future, FutureExt};
use tempfile::{self, TempDir};

use tokio::sync::mpsc::UnboundedReceiver;

use crate::assert_ok;

const CHANNEL_TIMEOUT: Duration = Duration::from_millis(10);

/// Checks if a given future either is ready, or will become ready on the next poll/without yielding
pub fn assert_future_can_complete<I>(f: impl Future<Output = I>) -> I {
	assert_ok!(f.now_or_never())
}

pub fn with_file<C: FnOnce(&Path)>(text: &[u8], closure: C) {
	let mut tempfile = tempfile::NamedTempFile::new().unwrap();
	tempfile.write_all(text).unwrap();
	closure(tempfile.path());
}

/// Create a temp directory that will be deleted when `TempDir` is dropped.
/// Also returns the path to a non-existent file in the directory.
pub fn new_temp_directory_with_nonexistent_file() -> (TempDir, PathBuf) {
	let tempdir = tempfile::TempDir::new().unwrap();
	let tempfile = tempdir.path().to_owned().join("file");
	assert!(!tempfile.exists());
	(tempdir, tempfile)
}

pub async fn recv_with_timeout<I>(receiver: &mut UnboundedReceiver<I>) -> Option<I> {
	tokio::time::timeout(CHANNEL_TIMEOUT, receiver.recv()).await.ok()?
}

pub async fn expect_recv_with_timeout<Item: std::fmt::Debug>(
	receiver: &mut UnboundedReceiver<Item>,
) -> Item {
	match recv_with_timeout(receiver).await {
		Some(i) => i,
		None => panic!("Timeout waiting for message, expected {}", std::any::type_name::<Item>()),
	}
}

use tracing::Level;
use tracing_subscriber::Layer;

/// Run at the start of a unit test to output all tracing logs in a CLI readable format.
/// Do not leave this in unit tests or it will panic when running more than one at a time.
// Allow dead code because this function is a unit test debugging tool.
#[allow(dead_code)]
pub fn init_test_logger() {
	use tracing_subscriber::{
		prelude::__tracing_subscriber_SubscriberExt, registry, util::SubscriberInitExt,
	};

	registry().with(TestLoggerLayer).try_init().expect("Failed to init the test logger, make you only run one test at a time with `init_test_logger`");
}

struct TestLoggerLayer;

/// A custom layer for tracing that makes the logs more readable on a CLI. Adds color, formatting
/// and a list of key/value pairs while not showing spans, timestamps and other clutter.
impl<S> Layer<S> for TestLoggerLayer
where
	S: tracing::Subscriber + for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
	fn on_event(
		&self,
		event: &tracing::Event<'_>,
		_ctx: tracing_subscriber::layer::Context<'_, S>,
	) {
		// Only log events from our code
		if !event.metadata().target().contains("chainflip_engine") {
			return
		}

		const KV_LIST_INDENT: &str = "    \x1b[0;34m|\x1b[0m";
		const LOCATION_INDENT: &str = "    \x1b[0;34m-->\x1b[0m";

		let mut visitor = CustomVisitor::default();
		event.record(&mut visitor);

		// Color code with level
		let level_color = match *event.metadata().level() {
			Level::ERROR => "[0;31m",
			Level::WARN => "[0;33m",
			Level::INFO => "[0;36m",
			Level::DEBUG => "[0;32m",
			Level::TRACE => "[0;35m",
		};

		// Print the readable log
		println!(
			"\x1b{level_color}[{}]\x1b[0m {} {}",
			event.metadata().level().as_str(),
			visitor.message,
			// Only show the tag if its not empty
			if visitor.tag.is_some() {
				format!("([{}], {})", visitor.tag.unwrap(), event.metadata().target())
			} else {
				format!("({})", event.metadata().target())
			}
		);

		// Print the location of the log call if its a Warning or above
		if matches!(*event.metadata().level(), Level::WARN | Level::ERROR) {
			println!(
				"{LOCATION_INDENT} {}:{}",
				event.metadata().file().unwrap(),
				event.metadata().line().unwrap()
			);
		}

		// Print the list of key values pairs attached to the event
		visitor.kv.iter().for_each(|(k, v)| {
			println!("{KV_LIST_INDENT} {k} = {v}");
		});

		// TODO: print the current span and any key values attached to it
	}
}

use std::{
	collections::HashMap,
	fmt::{self},
};
use tracing::field::{Field, Visit};

#[derive(Default)]
struct CustomVisitor {
	pub message: String,
	pub tag: Option<String>,
	pub kv: HashMap<String, String>,
}

// Gathers data from a log event
impl Visit for CustomVisitor {
	fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
		match field.name() {
			"message" => self.message = format!("{value:?}"),
			"tag" => self.tag = Some(format!("{value:?}")),
			_ => {
				self.kv.insert(field.name().to_string(), format!("{value:?}"));
			},
		}
	}
}
