use std::collections::VecDeque;

use futures::stream;
use futures_util::StreamExt;

use cf_chains::Chain;
use num_traits::CheckedSub;

use crate::witness::common::{chain_source::ChainClient, ExternalChainSource};

use super::{BoxChainStream, ChainSource, Header};

/// Outputs the block index that is `margin` less than the last block pulled from the inner source.
/// This means it's possible it produces two of the same block, for example, if strictly monotonic
/// is not applied before the lag_safety.
#[derive(Clone)]
pub struct LagSafety<InnerSource: ExternalChainSource> {
	inner_source: InnerSource,
	margin: <InnerSource::Chain as Chain>::ChainBlockNumber,
}
impl<InnerSource: ExternalChainSource> LagSafety<InnerSource> {
	pub fn new(
		inner_source: InnerSource,
		margin: <InnerSource::Chain as Chain>::ChainBlockNumber,
	) -> Self {
		Self { inner_source, margin }
	}
}

type ChainHeader<CS> =
	Header<<CS as ChainSource>::Index, <CS as ChainSource>::Hash, <CS as ChainSource>::Data>;

#[async_trait::async_trait]
impl<InnerSource: ExternalChainSource> ChainSource for LagSafety<InnerSource>
where
	InnerSource::Client: Clone,
{
	type Index = InnerSource::Index;
	type Hash = InnerSource::Hash;
	type Data = InnerSource::Data;

	type Client = InnerSource::Client;

	async fn stream_and_client(
		&self,
	) -> (BoxChainStream<'_, Self::Index, Self::Hash, Self::Data>, Self::Client) {
		let (chain_stream, chain_client) = self.inner_source.stream_and_client().await;

		let margin = self.margin;

		(
			Box::pin(stream::unfold(
				(chain_stream, chain_client.clone(), VecDeque::<ChainHeader<Self>>::new()),
				move |(mut chain_stream, chain_client, mut unsafe_cache)| async move {
					fn pop_safe_from_cache<CS: ExternalChainSource>(
						unsafe_cache: &mut VecDeque<ChainHeader<CS>>,
						margin: <CS as ChainSource>::Index,
					) -> Option<ChainHeader<CS>> {
						use num_traits::CheckedSub;

						if (*<CS::Chain as Chain>::block_witness_range(unsafe_cache.back()?.index)
							.end())
						.checked_sub(
							<CS::Chain as Chain>::block_witness_range(unsafe_cache.front()?.index)
								.end(),
						)? >= margin
						{
							Some(unsafe_cache.pop_front().unwrap())
						} else {
							None
						}
					}

					if let Some(safe_header) = pop_safe_from_cache::<Self>(&mut unsafe_cache, margin) {
						// Technically this check is unneeded as the witness_period is constant, but if it weren't then a new unsafe block could
						// cause multiple new blocks to become safe, in which case this would be needed.
						Some(safe_header)
					} else {
						utilities::loop_select!(
							if let Some(header) = chain_stream.next() => {
								let header_index = header.index;
								assert!(<InnerSource::Chain as Chain>::is_block_witness_root(header_index));
								if unsafe_cache.back().map_or(false, |last_header| Some(&last_header.hash) != header.parent_hash.as_ref()) {
									unsafe_cache.clear();
								}
								unsafe_cache.push_back(header);
								if let Some(safe_header) = pop_safe_from_cache::<Self>(&mut unsafe_cache, margin) {
									break Some(safe_header)
								} else if let Some(associated_safe_non_root_block) = <InnerSource::Chain as Chain>::block_witness_range(header_index).end().checked_sub(&margin) {
									// We don't check the sequence of hashes and assume due to order of requests it will be safe (even though this is not true)
									if *<InnerSource::Chain as Chain>::block_witness_range(associated_safe_non_root_block).end() == associated_safe_non_root_block {
										break Some(chain_client.header_at_index(<InnerSource::Chain as Chain>::block_witness_root(associated_safe_non_root_block)).await);
									} else if let Some(safe_root_block) = <InnerSource::Chain as Chain>::checked_block_witness_previous(associated_safe_non_root_block) {
										break Some(chain_client.header_at_index(safe_root_block).await);
									}
								}
							} else break None,
						)
					}.map(move |item| (item, (chain_stream, chain_client, unsafe_cache)))
				},
			)),
			chain_client,
		)
	}
}

impl<InnerSource: ExternalChainSource> ExternalChainSource for LagSafety<InnerSource>
where
	InnerSource::Client: Clone,
{
	type Chain = InnerSource::Chain;
}

#[cfg(test)]
mod tests {
	use sp_runtime::traits::One;
	use std::{ops::Range, sync::Arc};

	use crate::{common::Mutex, witness::common::chain_source::ChainClient};

	use super::*;

	use futures::Stream;

	#[derive(Clone)]
	pub struct MockChainClient<ExternalChain: Chain> {
		// These are the indices that have been queried, not got from the inner stream.
		queried_indices: Arc<Mutex<Vec<ExternalChain::ChainBlockNumber>>>,
	}

	impl<ExternalChain: Chain> MockChainClient<ExternalChain> {
		pub async fn queried_indices(&self) -> Vec<ExternalChain::ChainBlockNumber> {
			let guard = self.queried_indices.lock().await;
			guard.clone()
		}
	}

	#[async_trait::async_trait]
	impl<ExternalChain: Chain> ChainClient for MockChainClient<ExternalChain> {
		type Index = ExternalChain::ChainBlockNumber;
		type Hash = ExternalChain::ChainBlockNumber;
		type Data = ();

		async fn header_at_index(
			&self,
			index: Self::Index,
		) -> Header<Self::Index, Self::Hash, Self::Data> {
			let mut queried = self.queried_indices.lock().await;
			queried.push(index);
			Header {
				index,
				hash: index + ExternalChain::WITNESS_PERIOD - One::one(),
				parent_hash: Some(index - One::one()),
				data: (),
			}
		}
	}

	pub struct MockChainSource<
		ExternalChain: Chain,
		HeaderStream: Stream<
				Item = Header<ExternalChain::ChainBlockNumber, ExternalChain::ChainBlockNumber, ()>,
			> + Send
			+ Sync,
	> {
		stream: Arc<Mutex<Option<HeaderStream>>>,
		client: MockChainClient<ExternalChain>,
	}

	impl<
			ExternalChain: Chain,
			HeaderStream: Stream<
					Item = Header<
						ExternalChain::ChainBlockNumber,
						ExternalChain::ChainBlockNumber,
						(),
					>,
				> + Send
				+ Sync,
		> MockChainSource<ExternalChain, HeaderStream>
	{
		fn new(stream: HeaderStream) -> Self {
			Self {
				stream: Arc::new(Mutex::new(Some(stream))),
				client: MockChainClient { queried_indices: Arc::new(Mutex::new(Vec::new())) },
			}
		}
	}

	#[async_trait::async_trait]
	impl<
			ExternalChain: Chain,
			HeaderStream: Stream<
					Item = Header<
						ExternalChain::ChainBlockNumber,
						ExternalChain::ChainBlockNumber,
						(),
					>,
				> + Send
				+ Sync,
		> ChainSource for MockChainSource<ExternalChain, HeaderStream>
	{
		type Index = ExternalChain::ChainBlockNumber;
		type Hash = ExternalChain::ChainBlockNumber;
		type Data = ();

		type Client = MockChainClient<ExternalChain>;

		async fn stream_and_client(
			&self,
		) -> (BoxChainStream<'_, Self::Index, Self::Hash, Self::Data>, Self::Client) {
			let mut guard = self.stream.lock().await;
			let stream = guard.take().expect("should only be called once, with a stream set");
			(Box::pin(stream), self.client.clone())
		}
	}
	impl<
			ExternalChain: Chain,
			HeaderStream: Stream<
					Item = Header<
						ExternalChain::ChainBlockNumber,
						ExternalChain::ChainBlockNumber,
						(),
					>,
				> + Send
				+ Sync,
		> ExternalChainSource for MockChainSource<ExternalChain, HeaderStream>
	{
		type Chain = ExternalChain;
	}

	pub fn normal_header(index: u64) -> Header<u64, u64, ()> {
		Header { index, hash: index, parent_hash: Some(index - 1), data: () }
	}

	#[tokio::test]
	async fn empty_inner_stream_returns_empty_no_lag() {
		let mock_chain_source = MockChainSource::<cf_chains::Ethereum, _>::new(stream::empty());

		let lag_safety = LagSafety::new(mock_chain_source, 0);

		let (mut chain_stream, client) = lag_safety.stream_and_client().await;

		assert!(chain_stream.next().await.is_none());
		assert!(client.queried_indices().await.is_empty())
	}

	#[tokio::test]
	async fn empty_inner_stream_returns_empty_with_lag() {
		let mock_chain_source = MockChainSource::<cf_chains::Ethereum, _>::new(stream::empty());

		let lag_safety = LagSafety::new(mock_chain_source, 4);

		let (mut chain_stream, client) = lag_safety.stream_and_client().await;

		assert!(chain_stream.next().await.is_none());
		assert!(client.queried_indices().await.is_empty())
	}

	#[tokio::test]
	async fn no_margin_passes_through() {
		const INDICES: Range<u64> = 5u64..10;
		let mock_chain_source = MockChainSource::<cf_chains::Ethereum, _>::new(
			stream::iter(INDICES).map(normal_header),
		);

		let lag_safety = LagSafety::new(mock_chain_source, 0);

		let (mut chain_stream, client) = lag_safety.stream_and_client().await;

		for i in INDICES {
			assert_eq!(chain_stream.next().await.unwrap().index, i);
		}
		assert!(chain_stream.next().await.is_none());
		// If we pass the stream straight through, we don't need to query for any blocks.
		assert!(client.queried_indices().await.is_empty())
	}

	#[tokio::test]
	async fn margin_holds_up_blocks() {
		const INDICES: Range<u64> = 5u64..10;
		const MARGIN: u64 = 2;
		let mock_chain_source = MockChainSource::<cf_chains::Ethereum, _>::new(
			stream::iter(INDICES).map(normal_header),
		);
		let lag_safety = LagSafety::new(mock_chain_source, MARGIN);

		let (mut chain_stream, client) = lag_safety.stream_and_client().await;

		for i in (INDICES.start - MARGIN)..(INDICES.end - MARGIN) {
			assert_eq!(chain_stream.next().await.unwrap().index, i);
		}
		assert!(chain_stream.next().await.is_none());
		assert_eq!(
			client.queried_indices().await,
			(INDICES.start - MARGIN..INDICES.start).collect::<Vec<_>>()
		);
	}

	fn test_header(index: u64, hash: u64, parent_hash: u64) -> Header<u64, u64, ()> {
		Header { index, hash, parent_hash: Some(parent_hash), data: () }
	}

	// Normally this isn't going to occur because the strictly monotonic will be applied first.
	#[tokio::test]
	async fn duplicate_block_index_produces_duplicate_output_blocks() {
		const MARGIN: u64 = 1;

		// one block fork
		let mock_chain_source = MockChainSource::<cf_chains::Ethereum, _>::new(stream::iter([
			test_header(5, 5, 4),
			test_header(5, 55, 4),
		]));

		let lag_safety = LagSafety::new(mock_chain_source, MARGIN);

		let (mut chain_stream, _) = lag_safety.stream_and_client().await;

		assert_eq!(chain_stream.next().await, Some(test_header(4, 4, 3)));
		assert_eq!(chain_stream.next().await, Some(test_header(4, 4, 3)));
		assert!(chain_stream.next().await.is_none());
	}

	#[tokio::test]
	async fn reorg_with_depth_equal_to_safety_margin_queries_for_correct_blocks() {
		const MARGIN: u64 = 3;

		let mock_chain_source = MockChainSource::<cf_chains::Ethereum, _>::new(stream::iter([
			// these three are on a bad fork
			test_header(5, 55, 44),
			test_header(6, 66, 55),
			test_header(7, 77, 66),
			// canonical chain
			normal_header(8),
			normal_header(9),
			normal_header(10),
			normal_header(11),
			normal_header(12),
			normal_header(13),
		]));

		let lag_safety = LagSafety::new(mock_chain_source, MARGIN);
		let (mut chain_stream, client) = lag_safety.stream_and_client().await;

		for i in (5 - MARGIN)..=(13 - MARGIN) {
			assert_eq!(chain_stream.next().await, Some(normal_header(i)));
		}

		assert!(chain_stream.next().await.is_none());
		assert_eq!(client.queried_indices().await, vec![2, 3, 4, 5, 6, 7]);
	}

	// This is not ideal, but it's an accepted risk. We test this to ensure that we don't crash or
	// encounter some other strange, unaccounted for behaviour.
	#[tokio::test]
	async fn reorg_with_depth_less_than_safety_margin_passes_through_bad_block() {
		const MARGIN: u64 = 2;

		let bad_block = test_header(5, 55, 44);

		let mock_chain_source = MockChainSource::<cf_chains::Ethereum, _>::new(stream::iter([
			// these three are on a bad fork
			bad_block,
			test_header(6, 66, 55),
			test_header(7, 77, 66),
			// canonical chain
			normal_header(8),
			normal_header(9),
			normal_header(10),
			normal_header(11),
			normal_header(12),
		]));

		let lag_safety = LagSafety::new(mock_chain_source, MARGIN);
		let (mut chain_stream, client) = lag_safety.stream_and_client().await;

		for i in (5 - MARGIN)..5 {
			assert_eq!(chain_stream.next().await, Some(normal_header(i)));
		}
		// Here's the bad block, we consider it safe now, because within the bad fork it's safe.
		assert_eq!(chain_stream.next().await.unwrap(), bad_block);

		for i in 6..(12 - MARGIN) {
			assert_eq!(chain_stream.next().await, Some(normal_header(i)));
		}

		// NB: No 5, since we returned as safe on the bad fork.
		assert_eq!(client.queried_indices().await, vec![3, 4, 6, 7]);
	}

	#[tokio::test]
	async fn margin_functions_with_greater_than_one_witness_period() {
		async fn test_margin(
			margins: &[u64],
			expected: &[Header<u64, u64, ()>],
			expected_queries: &[u64],
		) {
			for margin in margins {
				let mock_chain_source =
					MockChainSource::<cf_chains::Arbitrum, _>::new(stream::iter(
						(72u64..=120).step_by(cf_chains::Arbitrum::WITNESS_PERIOD as usize).map(
							|index| {
								test_header(
									index,
									index + cf_chains::Arbitrum::WITNESS_PERIOD - 1,
									index - 1,
								)
							},
						),
					));

				let lag_safety = LagSafety::new(mock_chain_source, *margin);
				let (chain_stream, client) = lag_safety.stream_and_client().await;

				assert_eq!(&chain_stream.collect::<Vec<_>>().await[..], expected);

				assert_eq!(&client.queried_indices().await[..], expected_queries);
			}
		}

		test_margin(
			&[0],
			&[test_header(72, 95, 71), test_header(96, 119, 95), test_header(120, 143, 119)],
			&[],
		)
		.await;
		test_margin(
			&[1, 5, 23, 24],
			&[test_header(48, 71, 47), test_header(72, 95, 71), test_header(96, 119, 95)],
			&[48],
		)
		.await;
		test_margin(
			&[25, 36, 47, 48],
			&[test_header(24, 47, 23), test_header(48, 71, 47), test_header(72, 95, 71)],
			&[24, 48],
		)
		.await;
	}
}
