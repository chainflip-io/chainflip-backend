pub const ETH_STREAM_BEHIND: &str = "eth-stream-behind";

use tracing::{metadata::LevelFilter, Level};
use tracing_subscriber::{EnvFilter, Layer};

/// Install global collector using json formatting and the RUST_LOG env var.
/// If `RUST_LOG` is not set, then it will default to INFO log level.
pub fn init_json_logger() {
	tracing_subscriber::fmt()
		.json()
		.with_env_filter(
			EnvFilter::builder()
				.with_default_directive(LevelFilter::INFO.into())
				.from_env_lossy(),
		)
		.init();
}

/// Run at the start of a unit test to output all tracing logs in a CLI readable format.
/// Do not leave this in unit tests or it will panic when running more than one at a time.
// Allow dead code because this function is a unit test debugging tool.
#[allow(dead_code)]
#[cfg(test)]
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
