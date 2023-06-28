use std::time::Duration;

use bitcoin::BlockHash;
use futures_util::stream;

use super::{ChainSource, Header};
use crate::{btc::retry_rpc::BtcRetryRpcApi, witness::chain_source::BoxChainStream};

pub struct BtcBlockStream<C: BtcRetryRpcApi> {
	rpc_client: C,
}

const POLL_INTERVAL: Duration = Duration::from_secs(10);

#[async_trait::async_trait]
impl<C: BtcRetryRpcApi + Clone + Send + Sync> ChainSource for BtcBlockStream<C> {
	type Index = u64;
	type Hash = BlockHash;
	type Data = ();

	async fn stream(&self) -> BoxChainStream<'_, Self::Index, Self::Hash, Self::Data> {
		Box::pin(stream::unfold(
			(self.rpc_client.clone(), None),
			|(client, last_block_hash_yielded)| async move {
				loop {
					tokio::time::sleep(POLL_INTERVAL).await;
					let best_block_hash = client.best_block_hash().await;
					if last_block_hash_yielded.is_some_and(|hash| best_block_hash != hash) ||
						last_block_hash_yielded.is_none()
					{
						let header = client.block_header(best_block_hash).await;
						assert_eq!(header.hash, best_block_hash);
						return Some((
							Header {
								index: header.height.into(),
								hash: header.hash,
								parent_hash: header.previous_block_hash,
								data: (),
							},
							(client, Some(best_block_hash)),
						))
					}
				}
			},
		))
	}
}
