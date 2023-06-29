use sp_core::H256;

use crate::eth::retry_rpc::EthersRetrySubscribeApi;
use ethers::prelude::*;
use futures::{stream::StreamExt, Stream};
use futures_util::stream;
use std::{pin::Pin, time::Duration};

use super::{BoxChainStream, ChainSource, Header};

pub struct EthSource<C: EthersRetrySubscribeApi> {
	client: C,
}

impl<C: EthersRetrySubscribeApi> EthSource<C> {
	pub fn new(client: C) -> Self {
		Self { client }
	}
}

const TIMEOUT: Duration = Duration::from_secs(60);
const RESTART_STREAM_DELAY: Duration = Duration::from_secs(6);

// #[async_trait::async_trait]
// impl<C: EthersRetrySubscribeApi + Clone + Send + Sync> ChainSource for EthSource<C> {
// 	type Index = u64;
// 	type Hash = H256;
// 	type Data = ();

// 	async fn stream(&self) -> BoxChainStream<'_, Self::Index, Self::Hash, Self::Data> {
// 		pub struct State<'a, C> {
// 			client: C,
// 			stream: SubscriptionStream<'a, Ws, Block<H256>>,
// 		}

// 		let client = self.client.clone();
// 		let stream = client.subscribe_blocks().await;
// 		Box::pin(stream::unfold(State { client, stream }, |mut state| async move {
// 			loop {
// 				while let Ok(Some(header)) =
// 					tokio::time::timeout(TIMEOUT, state.stream.next()).await
// 				{
// 					return Some((
// 						Header {
// 							index: header.number.unwrap().as_u64(),
// 							hash: header.hash.unwrap(),
// 							parent_hash: Some(header.parent_hash),
// 							data: (),
// 						},
// 						state,
// 					))
// 				}

// 				// We don't want to spam retries if the node returns a stream that's empty
// 				// immediately.
// 				tokio::time::sleep(RESTART_STREAM_DELAY).await;
// 				let stream = state.client.subscribe_blocks().await;
// 				state = State { client: state.client, stream };
// 			}
// 		}))
// 	}
// }
