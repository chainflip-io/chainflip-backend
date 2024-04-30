use cf_chains::evm::EvmCrypto;
use ethers::types::Bloom;
use futures::stream::StreamExt;
use futures_util::stream;
use sp_core::H256;

use crate::{
	evm::{
		core_h256,
		retry_rpc::{EvmRetryRpcApi, EvmRetrySubscribeApi},
		ConscientiousEvmWebsocketBlockHeaderStream,
	},
	witness::common::{
		chain_source::{BoxChainStream, ChainClient, ChainSource, Header},
		ExternalChain, ExternalChainSource,
	},
};
use std::{collections::VecDeque, time::Duration};

#[derive(Clone)]
pub struct EvmSource<Client, EvmChain> {
	client: Client,
	_phantom: std::marker::PhantomData<EvmChain>,
}

impl<C, EvmChain> EvmSource<C, EvmChain>
where
	EvmChain: ExternalChain<ChainCrypto = EvmCrypto>,
	C: EvmRetryRpcApi
		+ EvmRetrySubscribeApi
		+ ChainClient<Index = u64, Hash = H256, Data = Bloom>
		+ Clone,
{
	pub fn new(client: C) -> Self {
		Self { client, _phantom: std::marker::PhantomData }
	}
}

/// The maximum amount of time we wait for a block to be pulled from the stream.
const BLOCK_PULL_TIMEOUT: Duration = Duration::from_secs(60);

/// The time we wait before restarting the stream if we didn't get a block.
const RESTART_STREAM_DELAY: Duration = Duration::from_secs(6);

#[async_trait::async_trait]
impl<C, EvmChain> ChainSource for EvmSource<C, EvmChain>
where
	EvmChain: ExternalChain<ChainCrypto = EvmCrypto, ChainBlockNumber = u64>,
	C: EvmRetryRpcApi
		+ EvmRetrySubscribeApi
		+ ChainClient<Index = u64, Hash = H256, Data = Bloom>
		+ Clone,
{
	type Index = <C as ChainClient>::Index;
	type Hash = <C as ChainClient>::Hash;
	type Data = <C as ChainClient>::Data;
	type Client = C;

	async fn stream_and_client(
		&self,
	) -> (BoxChainStream<'_, Self::Index, Self::Hash, Self::Data>, Self::Client) {
		pub struct State<C> {
			client: C,
			stream: ConscientiousEvmWebsocketBlockHeaderStream,
			previous_block_sequence: VecDeque<Header<u64, H256, Bloom>>,
		}

		let client = self.client.clone();
		let stream = client.subscribe_blocks().await;
		(
			Box::pin(stream::unfold(
				State { client, stream, previous_block_sequence: Default::default() },
				|mut state| async move {
					loop {
						while let Ok(Some(header)) =
							tokio::time::timeout(BLOCK_PULL_TIMEOUT, state.stream.next()).await
						{
							if let Ok(header) = header {
								let (Some(index), Some(hash), parent_hash) = (
									header.number.map(|number| number.as_u64()),
									header.hash.map(core_h256),
									core_h256(header.parent_hash),
								) else {
									continue
								};

								let header = Header {
									index,
									hash,
									parent_hash: if index == 0 { None } else { Some(parent_hash) },
									data: header.logs_bloom,
								};

								if state.previous_block_sequence.back().map_or(
									false,
									|previous_header| {
										Some(previous_header.hash) != header.parent_hash
									},
								) {
									state.previous_block_sequence.clear();
								}
								state.previous_block_sequence.push_back(header);
								if state.previous_block_sequence.len() >
									EvmChain::WITNESS_PERIOD as usize
								{
									state.previous_block_sequence.pop_front();
									assert_eq!(
										state.previous_block_sequence.len(),
										EvmChain::WITNESS_PERIOD as usize
									);
								}

								if state.previous_block_sequence.len() >=
									EvmChain::WITNESS_PERIOD as usize
								{
									if let Some(header) = state
										.previous_block_sequence
										.front()
										.filter(|header| EvmChain::block_phase(header.index) == 0)
									{
										return Some((
											Header {
												index: header.index,
												hash: state.previous_block_sequence
													[EvmChain::WITNESS_PERIOD as usize - 1]
													.hash,
												parent_hash: header.parent_hash,
												data: header.data,
											},
											state,
										))
									}
								}
							}
						}

						// We don't want to spam retries if the node returns a stream that's empty
						// immediately.
						tokio::time::sleep(RESTART_STREAM_DELAY).await;
						state.stream = state.client.subscribe_blocks().await;
					}
				},
			)),
			self.client.clone(),
		)
	}
}

impl<C, EvmChain> ExternalChainSource for EvmSource<C, EvmChain>
where
	EvmChain: ExternalChain<ChainBlockNumber = u64, ChainCrypto = EvmCrypto>,
	C: EvmRetryRpcApi
		+ EvmRetrySubscribeApi
		+ ChainClient<Index = u64, Hash = H256, Data = Bloom>
		+ Clone,
{
	type Chain = EvmChain;
}
