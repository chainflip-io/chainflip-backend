use std::pin::Pin;

use futures::Stream;

use crate::witnesser::{block_head_stream_from::block_head_stream_from, BlockNumberable};

use anyhow::Result;

#[derive(Debug, Clone, Copy)]
pub struct MiniHeader {
	pub block_number: u64,
}

impl BlockNumberable for MiniHeader {
	fn block_number(&self) -> u64 {
		self.block_number
	}
}

pub async fn dot_block_head_stream_from<BlockHeaderStream>(
	from_block: u64,
	safe_head_stream: BlockHeaderStream,
	logger: &slog::Logger,
) -> Result<Pin<Box<dyn Stream<Item = MiniHeader> + Send + 'static>>>
where
	BlockHeaderStream: Stream<Item = MiniHeader> + 'static + Send,
{
	block_head_stream_from(
		from_block,
		safe_head_stream,
		move |block_number| Box::pin(async move { Ok(MiniHeader { block_number }) }),
		logger,
	)
	.await
}

#[cfg(test)]
mod tests {

	use super::*;

	use futures::StreamExt;
	use subxt::{OnlineClient, PolkadotConfig};

	use crate::{
		logging::test_utils::new_test_logger,
		settings::{CfSettings, CommandLineOptions, Settings},
	};

	#[tokio::test]
	#[ignore = "requires connecting to a live dotsama network"]
	async fn block_head_stream_from_test_dot() {
		let settings = Settings::load_settings_from_all_sources(
			"./config/Local.toml",
			None,
			CommandLineOptions::default(),
		)
		.unwrap();

		println!("Connecting to: {}", settings.dot.ws_node_endpoint);

		let safe_head_stream =
			OnlineClient::<PolkadotConfig>::from_url(settings.dot.ws_node_endpoint)
				.await
				.unwrap()
				.rpc()
				.subscribe_finalized_blocks()
				.await
				.unwrap()
				.map(|header| MiniHeader { block_number: header.unwrap().number.into() });

		let mut block_head_stream_from =
			dot_block_head_stream_from(13141870, safe_head_stream, &new_test_logger())
				.await
				.unwrap();

		while let Some(mini_blockhead) = block_head_stream_from.next().await {
			println!("Here's the mini_blockhead number: {:?}", mini_blockhead.block_number);
		}
	}
}
