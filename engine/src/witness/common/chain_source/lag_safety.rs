use std::{collections::VecDeque, iter::Step};

use futures::stream;
use futures_util::StreamExt;

use crate::witness::common::{chain_source::ChainClient, ExternalChainSource};

use super::{BoxChainStream, ChainSource, Header};

/// Outputs the block index that is `margin` less than the last block pulled from the inner source.
/// This means it's possible it produces two of the same block, for example, if strictly monotonic
/// is not applied before the lag_safety.
#[derive(Clone)]
pub struct LagSafety<InnerSource> {
	inner_source: InnerSource,
	margin: usize,
}
impl<InnerSource> LagSafety<InnerSource> {
	pub fn new(inner_source: InnerSource, margin: usize) -> Self {
		Self { inner_source, margin }
	}
}

#[async_trait::async_trait]
impl<InnerSource: ChainSource> ChainSource for LagSafety<InnerSource>
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
				(
					chain_stream,
					chain_client.clone(),
					VecDeque::<
						Header<
							<Self as ChainSource>::Index,
							<Self as ChainSource>::Hash,
							<Self as ChainSource>::Data,
						>,
					>::new(),
				),
				move |(mut chain_stream, chain_client, mut unsafe_cache)| async move {
					utilities::loop_select!(
						if let Some(header) = chain_stream.next() => {
							let header_index = header.index;
							if unsafe_cache.back().map_or(false, |last_header| Some(&last_header.hash) != header.parent_hash.as_ref() || Step::forward_checked(last_header.index, 1) != Some(header_index)) {
								unsafe_cache.clear();
							}
							unsafe_cache.push_back(header);
							if let Some(next_output_index) = Step::backward_checked(header_index, margin) {
								break Some(if unsafe_cache.len() > margin {
									assert_eq!(unsafe_cache.len() - 1, margin);
									unsafe_cache.pop_front().unwrap()
								} else {
									// We don't check sequence of hashes and assume due to order of requests it will be safe (even though this is not true)
									chain_client.header_at_index(next_output_index).await
								})
							}
						} else break None,
					).map(move |item| (item, (chain_stream, chain_client, unsafe_cache)))
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
	use std::{ops::Range, sync::Arc};

	use crate::common::Mutex;

	use super::*;

	use futures::Stream;

	#[derive(Clone)]
	pub struct MockChainClient {
		// These are the indices that have been queried, not got from the inner stream.
		queried_indices: Arc<Mutex<Vec<u64>>>,
	}

	impl MockChainClient {
		pub async fn queried_indices(&self) -> Vec<u64> {
			let guard = self.queried_indices.lock().await;
			guard.clone()
		}
	}

	#[async_trait::async_trait]
	impl ChainClient for MockChainClient {
		type Index = u64;

		type Hash = u64;

		type Data = ();

		async fn header_at_index(
			&self,
			index: Self::Index,
		) -> Header<Self::Index, Self::Hash, Self::Data> {
			let mut queried = self.queried_indices.lock().await;
			queried.push(index);
			Header { index, hash: index, parent_hash: Some(index - 1), data: () }
		}
	}

	pub struct MockChainSource<HeaderStream: Stream<Item = Header<u64, u64, ()>> + Send + Sync> {
		stream: Arc<Mutex<Option<HeaderStream>>>,
		client: MockChainClient,
	}

	impl<HeaderStream: Stream<Item = Header<u64, u64, ()>> + Send + Sync>
		MockChainSource<HeaderStream>
	{
		fn new(stream: HeaderStream) -> Self {
			Self {
				stream: Arc::new(Mutex::new(Some(stream))),
				client: MockChainClient { queried_indices: Arc::new(Mutex::new(Vec::new())) },
			}
		}
	}

	#[async_trait::async_trait]
	impl<HeaderStream: Stream<Item = Header<u64, u64, ()>> + Send + Sync> ChainSource
		for MockChainSource<HeaderStream>
	{
		type Index = u64;
		type Hash = u64;
		type Data = ();

		type Client = MockChainClient;

		async fn stream_and_client(
			&self,
		) -> (BoxChainStream<'_, Self::Index, Self::Hash, Self::Data>, Self::Client) {
			let mut guard = self.stream.lock().await;
			let stream = guard.take().unwrap();
			(Box::pin(stream), self.client.clone())
		}
	}

	pub fn normal_header(index: u64) -> Header<u64, u64, ()> {
		Header { index, hash: index, parent_hash: Some(index - 1), data: () }
	}

	#[tokio::test]
	async fn empty_inner_stream_returns_empty_no_lag() {
		let mock_chain_source = MockChainSource::new(stream::empty());

		let lag_safety = LagSafety::new(mock_chain_source, 0);

		let (mut chain_stream, client) = lag_safety.stream_and_client().await;

		assert!(chain_stream.next().await.is_none());
		assert!(client.queried_indices().await.is_empty())
	}

	#[tokio::test]
	async fn empty_inner_stream_returns_empty_with_lag() {
		let mock_chain_source = MockChainSource::new(stream::empty());

		let lag_safety = LagSafety::new(mock_chain_source, 4);

		let (mut chain_stream, client) = lag_safety.stream_and_client().await;

		assert!(chain_stream.next().await.is_none());
		assert!(client.queried_indices().await.is_empty())
	}

	#[tokio::test]
	async fn no_margin_passes_through() {
		const INDICES: Range<u64> = 5u64..10;
		let mock_chain_source = MockChainSource::new(stream::iter(INDICES).map(normal_header));

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
		const MARGIN: usize = 2;
		let mock_chain_source = MockChainSource::new(stream::iter(INDICES).map(normal_header));
		let lag_safety = LagSafety::new(mock_chain_source, MARGIN);

		let (mut chain_stream, client) = lag_safety.stream_and_client().await;

		for i in (INDICES.start - MARGIN as u64)..(INDICES.end - MARGIN as u64) {
			assert_eq!(chain_stream.next().await.unwrap().index, i);
		}
		assert!(chain_stream.next().await.is_none());
		assert_eq!(client.queried_indices().await, vec![3, 4]);
	}

	fn test_header(index: u64, hash: u64, parent_hash: u64) -> Header<u64, u64, ()> {
		Header { index, hash, parent_hash: Some(parent_hash), data: () }
	}

	// Normally this isn't going to occur because the strictly monotonic will be applied first.
	#[tokio::test]
	async fn duplicate_block_index_produces_duplicate_output_blocks() {
		const MARGIN: usize = 1;

		// one block fork
		let mock_chain_source =
			MockChainSource::new(stream::iter([test_header(5, 5, 4), test_header(5, 55, 4)]));

		let lag_safety = LagSafety::new(mock_chain_source, MARGIN);

		let (mut chain_stream, _) = lag_safety.stream_and_client().await;

		assert_eq!(chain_stream.next().await, Some(test_header(4, 4, 3)));
		assert_eq!(chain_stream.next().await, Some(test_header(4, 4, 3)));
		assert!(chain_stream.next().await.is_none());
	}

	#[tokio::test]
	async fn reorg_with_depth_equal_to_safety_margin_queries_for_correct_blocks() {
		const MARGIN: usize = 3;

		let mock_chain_source = MockChainSource::new(stream::iter([
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

		for i in (5 - MARGIN as u64)..=(13 - MARGIN as u64) {
			assert_eq!(chain_stream.next().await, Some(normal_header(i)));
		}

		assert!(chain_stream.next().await.is_none());
		assert_eq!(client.queried_indices().await, vec![2, 3, 4, 5, 6, 7]);
	}

	// This is not ideal, but it's an accepted risk. We test this to ensure that we don't crash or
	// some other strange behaviour.
	#[tokio::test]
	async fn reorg_with_depth_less_than_safety_margin_passes_through_bad_block() {
		const MARGIN: usize = 2;

		let mock_chain_source = MockChainSource::new(stream::iter([
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
		]));

		let lag_safety = LagSafety::new(mock_chain_source, MARGIN);
		let (mut chain_stream, client) = lag_safety.stream_and_client().await;

		for i in (5 - MARGIN as u64)..(5 as u64) {
			assert_eq!(chain_stream.next().await, Some(normal_header(i)));
		}
		// Here's the bad block, we consider it safe now, because within the bad fork it's safe.
		assert_eq!(chain_stream.next().await.unwrap(), test_header(5, 55, 44));

		for i in (6 as u64)..(12 - MARGIN as u64) {
			assert_eq!(chain_stream.next().await, Some(normal_header(i)));
		}

		// NB: No 5, since we returned as safe on the bad fork.
		assert_eq!(client.queried_indices().await, vec![3, 4, 6, 7]);
	}
}
