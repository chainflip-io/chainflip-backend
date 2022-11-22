use std::{collections::BTreeSet, pin::Pin, sync::Arc};

use cf_chains::dot::Polkadot;
use futures::Stream;
use subxt::{Config, PolkadotConfig};

use crate::witnesser::{
	block_head_stream_from::block_head_stream_from, epoch_witnesser, BlockNumberable, EpochStart,
};

use anyhow::Result;

type PolkadotAccount = <PolkadotConfig as Config>::AccountId;

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

pub async fn start<StateChainClient>(
	epoch_starts_receiver: async_broadcast::Receiver<EpochStart<Polkadot>>,
	dot_monitor_ingress_receiver: tokio::sync::mpsc::UnboundedReceiver<PolkadotAccount>,
	_state_chain_client: Arc<StateChainClient>,
	// on chain addresses that we need to monitor for inputs
	monitored_addresses: BTreeSet<PolkadotAccount>,
	logger: &slog::Logger,
) -> Result<()> {
	epoch_witnesser::start(
		"DOT".to_string(),
		epoch_starts_receiver,
		|_epoch_start| true,
		(monitored_addresses, dot_monitor_ingress_receiver),
		move |_end_witnessing_signal,
		      _epoch_start,
		      (monitored_addresses, dot_monitor_ingress_receiver),
		      _logger| { async move { Ok((monitored_addresses, dot_monitor_ingress_receiver)) } },
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
			"/etc/chainflip_local_custom/".to_owned(),
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
