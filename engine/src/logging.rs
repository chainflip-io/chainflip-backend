pub const ETH_STREAM_BEHIND: &str = "eth-stream-behind";

use tracing::metadata::LevelFilter;
use tracing_subscriber::EnvFilter;

#[cfg(test)]
pub use utilities::testing::init_test_logger;

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
