use std::{collections::VecDeque, iter::Step};

use futures::stream;
use futures_util::StreamExt;

use crate::witness::chain_source::ChainClient;

use super::{BoxChainStream, ChainSource, Header};

pub struct LagSafety<Inner: ChainSource> {
	inner: Inner,
	margin: usize,
}
impl<Inner: ChainSource> LagSafety<Inner> {
	pub fn new(margin: usize, inner: Inner) -> Self {
		Self { inner, margin }
	}
}

#[async_trait::async_trait]
impl<Inner: ChainSource> ChainSource for LagSafety<Inner>
where
	Inner::Client: Clone,
{
	type Index = Inner::Index;
	type Hash = Inner::Hash;
	type Data = Inner::Data;

	type Client = Inner::Client;

	async fn stream_and_client(
		&self,
	) -> (BoxChainStream<'_, Self::Index, Self::Hash, Self::Data>, Self::Client) {
		let (chain_stream, chain_client) = self.inner.stream_and_client().await;
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
							if unsafe_cache.back().map_or(false, |last_header| Some(&last_header.hash) != header.parent_hash.as_ref() || Step::forward_checked(last_header.index, 1) != Some(header.index)) {
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
