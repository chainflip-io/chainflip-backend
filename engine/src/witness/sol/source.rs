use futures::stream::StreamExt;
use futures_util::stream;
use sp_core::H256;
use utilities::make_periodic_tick;

use crate::{
	sol::retry_rpc::SolRetryRpcApi,
	witness::common::{
		chain_source::{BoxChainStream, ChainClient, ChainSource, Header},
		ExternalChain, ExternalChainSource,
	},
};
use cf_chains::sol::SolHash;
use std::{collections::VecDeque, time::Duration};

#[derive(Clone)]
pub struct SolSource<Client> {
	client: Client,
}

impl<C> SolSource<C> {
	pub fn new(client: C) -> Self {
		Self { client }
	}
}

const POLL_INTERVAL: Duration = Duration::from_secs(5);

#[async_trait::async_trait]
impl<C> ChainSource for SolSource<C>
where
	C: SolRetryRpcApi + ChainClient<Index = u64, Hash = SolHash, Data = ()> + Clone,
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
				// TODO: Write this code for Solana. Something should be related to the witness
				// period?
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

impl<C> ExternalChainSource for SolSource<C>
where
	C: SolRetryRpcApi + ChainClient<Index = u64, Hash = SolHash, Data = ()> + Clone,
{
	type Chain = cf_chains::sol::Solana;
}
