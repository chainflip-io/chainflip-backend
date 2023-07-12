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
							(epoch.clone(), chain_stream, None),
							move |(epoch, mut chain_stream, mut option_pending_then)| {
								async move {
									// skip forward to newest header
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
										let apply_then = |header: Header<_, _, _>| {
											let epoch = epoch.clone();
											async move {
												Header {
													index: header.index,
													hash: header.hash,
													parent_hash: header.parent_hash,
													data: (self.then_fn)(epoch, header).await,
												}
											}
											.boxed()
										};

										let mut pending_then = apply_then(header);

										// We use two strategies concurrently:
										loop_select!(
											// We take an item and run then_fn for it, and wait for that to finish before starting a new then_fn run
											let mapped_header = &mut pending_then => {
												// We keep the future from the cancelling strategy (option_pending_then), so we can continue running it
												break Some((mapped_header, (epoch, chain_stream, option_pending_then)))
											},
											// We take an item and run then_fn for it, and cancel it if the stream produces a new header (and then run then_fn for that new header)
											if let Some(new_header) = chain_stream.next() => {
												option_pending_then = Some(apply_then(new_header));
											} else break None,
											if option_pending_then.is_some() => let mapped_header = option_pending_then.as_mut().unwrap() => {
												break Some((mapped_header, (epoch, chain_stream, None)))
											},
										)
									} else {
										None
									}
								}
								.boxed()
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
