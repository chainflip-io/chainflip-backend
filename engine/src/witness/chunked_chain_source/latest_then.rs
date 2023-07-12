use futures::future;
use futures_core::Future;
use futures_util::{stream, FutureExt, StreamExt};
use utilities::loop_select;

use crate::witness::{
	chain_source::{aliases, ChainStream, Header},
	common::BoxActiveAndFuture,
	epoch_source::Epoch,
};

use super::{then::ThenClient, ChunkedChainSource};

pub struct LatestThen<Inner, ThenFn> {
	inner: Inner,
	then_fn: ThenFn,
}
impl<Inner, ThenFn> LatestThen<Inner, ThenFn> {
	pub fn new(inner: Inner, then_fn: ThenFn) -> Self {
		Self { inner, then_fn }
	}
}
#[async_trait::async_trait]
impl<Inner: ChunkedChainSource, Output, Fut, ThenFn> ChunkedChainSource
	for LatestThen<Inner, ThenFn>
where
	Output: aliases::Data,
	Fut: Future<Output = Output> + Send,
	ThenFn: Fn(
			Epoch<Inner::Info, Inner::HistoricInfo>,
			Header<Inner::Index, Inner::Hash, Inner::Data>,
		) -> Fut
		+ Send
		+ Sync
		+ Clone,
{
	type Info = Inner::Info;
	type HistoricInfo = Inner::HistoricInfo;

	type Index = Inner::Index;
	type Hash = Inner::Hash;
	type Data = Output;

	type Client = ThenClient<Inner, ThenFn>;

	type Chain = Inner::Chain;

	type Parameters = Inner::Parameters;

	async fn stream(
		&self,
		parameters: Self::Parameters,
	) -> BoxActiveAndFuture<'_, super::Item<'_, Self, Self::Info, Self::HistoricInfo>> {
		self.inner
			.stream(parameters)
			.await
			.then(move |(epoch, chain_stream, chain_client)| {
				async move {
					(
						epoch.clone(),
						stream::unfold(
							(epoch.clone(), chain_stream),
							move |(epoch, mut chain_stream)| {
								async move {
									let option_header = if let Some(option_header) =
										std::iter::repeat(())
											.map_while(|_| chain_stream.next().now_or_never())
											.last()
									{
										option_header
									} else {
										chain_stream.next().await
									};

									if let Some(header) = option_header {
										let option_then_header = future::select(
											async {
												// This doesn't get cancelled by new items to ensure
												// stream will produce items even if the inner
												// stream produces items faster than then_fn can run
												Some(Header {
													index: header.index,
													hash: header.hash,
													parent_hash: header.parent_hash,
													data: (self.then_fn)(epoch.clone(), header)
														.await,
												})
											}
											.boxed(),
											async {
												if let Some(mut header) = chain_stream.next().await
												{
													loop_select!(
														if let Some(new_header) = chain_stream.next() => {
															header = new_header;
														} else break None,
														let then_header = async {
															Header {
																index: header.index,
																hash: header.hash,
																parent_hash: header.parent_hash,
																data: (self.then_fn)(epoch.clone(), header).await,
															}
														} => {
															break Some(then_header)
														},
													)
												} else {
													// If inner stream ends the outer stream will
													// end, even if there are currently running
													// then_fn instances
													None
												}
											}
											.boxed(),
										)
										.await
										.factor_first()
										.0;

										option_then_header
											.map(move |header| (header, (epoch, chain_stream)))
									} else {
										None
									}
								}
							},
						)
						.into_box(),
						ThenClient::new(chain_client, self.then_fn.clone(), epoch),
					)
				}
			})
			.await
			.into_box()
	}
}
