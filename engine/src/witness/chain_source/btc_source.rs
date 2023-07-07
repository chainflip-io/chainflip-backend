use std::time::Duration;

use bitcoin::BlockHash;
use futures_util::stream;
use utilities::make_periodic_tick;

use super::{ChainClient, ChainSourceWithClient, Header};
use crate::{btc::retry_rpc::BtcRetryRpcApi, witness::chain_source::BoxChainStream};

pub struct BtcBlockStream<C: BtcRetryRpcApi> {
	client: C,
}

const POLL_INTERVAL: Duration = Duration::from_secs(10);

#[async_trait::async_trait]
impl<C> ChainSourceWithClient for BtcBlockStream<C>
where
	C: BtcRetryRpcApi + ChainClient<Index = u64, Hash = BlockHash, Data = ()> + Clone,
{
	type Index = <C as ChainClient>::Index;
	type Hash = <C as ChainClient>::Hash;
	type Data = <C as ChainClient>::Data;
	type Client = C;

	async fn stream_and_client(
		&self,
	) -> (BoxChainStream<'_, Self::Index, Self::Hash, Self::Data>, Self::Client) {
		(
			Box::pin(stream::unfold(
				(self.client.clone(), None, make_periodic_tick(POLL_INTERVAL, true)),
				|(client, last_block_hash_yielded, mut tick)| async move {
					loop {
						tick.tick().await;

						let best_block_header = client.best_block_header().await;
						if last_block_hash_yielded != Some(best_block_header.hash) {
							return Some((
								Header {
									index: best_block_header.height,
									hash: best_block_header.hash,
									parent_hash: best_block_header.previous_block_hash,
									data: (),
								},
								(client, Some(best_block_header.hash), tick),
							))
						}
					}
				},
			)),
			self.client.clone(),
		)
	}
}
