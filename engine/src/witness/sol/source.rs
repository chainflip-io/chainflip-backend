use futures_util::stream;
use utilities::make_periodic_tick;

use crate::{
	sol::{commitment_config::CommitmentConfig, retry_rpc::SolRetryRpcApi},
	witness::common::{
		chain_source::{BoxChainStream, ChainClient, ChainSource, Header},
		ExternalChainSource,
	},
};
use cf_chains::{sol::SolHash, Chain, Solana};
use std::time::Duration;

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
				|(client, last_witnessed_range_end, mut tick)| async move {
					loop {
						tick.tick().await;

						let slot = client.get_slot(CommitmentConfig::finalized()).await;

						// Return a maximum of one header per range
						if Some(slot) > last_witnessed_range_end {
							let witness_range = Solana::block_witness_range(slot);

							let witness_range_end = *witness_range.end();
							return Some((
								Header {
									index: Solana::block_witness_root(slot),
									hash: SolHash::default(),
									parent_hash: None,
									data: (),
								},
								(client, Some(witness_range_end), tick),
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
