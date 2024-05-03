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

/// Note this produces Header's where the hash does not necessarily correspond to real EVM blocks,
/// if the WITNESS_PERIOD is more than 1. In that case the hash will be the hash of the last block
/// in the witness range, instead of the hash of the block number equal to the Header's index.
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
			evm_header_sequence: VecDeque<Header<u64, H256, Bloom>>,
		}

		let client = self.client.clone();
		let stream = client.subscribe_blocks().await;
		(
			Box::pin(stream::unfold(
				State { client, stream, evm_header_sequence: Default::default() },
				|mut state| async move {
					loop {
						while let Ok(Some(result_raw_evm_header)) =
							tokio::time::timeout(BLOCK_PULL_TIMEOUT, state.stream.next()).await
						{
							if let Some(evm_header) =
								result_raw_evm_header.ok().and_then(|raw_evm_header| {
									let index =
										raw_evm_header.number.map(|number| number.as_u64())?;
									Some(Header {
										index,
										hash: raw_evm_header.hash.map(core_h256)?,
										parent_hash: if index == 0 {
											None
										} else {
											Some(core_h256(raw_evm_header.parent_hash))
										},
										data: raw_evm_header.logs_bloom,
									})
								}) {
								if state.evm_header_sequence.back().map_or(
									false,
									|previous_evm_header| {
										Some(previous_evm_header.hash) != evm_header.parent_hash
									},
								) {
									state.evm_header_sequence.clear();
								}
								state.evm_header_sequence.push_back(evm_header);

								let witness_range = EvmChain::block_witness_range(evm_header.index);

								if *witness_range.end() == evm_header.index {
									if let Some(first_evm_header_in_range) =
										state.evm_header_sequence.iter().find(|evm_header| {
											evm_header.index == *witness_range.start()
										}) {
										let composite_header = Header {
											index: EvmChain::block_witness_root(evm_header.index),
											hash: evm_header.hash,
											parent_hash: first_evm_header_in_range.parent_hash,
											data: evm_header.data,
										};
										state.evm_header_sequence.clear();
										return Some((composite_header, state))
									} else {
										state.evm_header_sequence.clear();
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
