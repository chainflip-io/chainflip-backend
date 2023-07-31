use futures_core::Future;
use futures_util::{stream, FutureExt, StreamExt};
use utilities::loop_select;

use crate::witness::common::{
	chain_source::{aliases, ChainStream, Header},
	epoch_source::Epoch,
	BoxActiveAndFuture,
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
							move |(epoch, mut chain_stream, option_old_then_fut)| {
								async move {
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

									let (
										// The future for the first header we see
										mut option_first_then_fut,
										// The future for the newest header we've seen
										mut option_newest_then_fut
									) = {
										// skip forward to newest ready item
										let option_latest_fut = {
											let mut option_latest = None;
											loop {
												match chain_stream.next().now_or_never() {
													Some(None) => return None,
													Some(Some(item)) => option_latest = Some(item),
													None => break option_latest
												}
											}
										}.map(apply_then);

										match option_old_then_fut {
											Some(old_then_fut) => (Some(old_then_fut), option_latest_fut),
											None => (option_latest_fut, None),
										}
									};

									loop_select!(
										if let Some(newest_header) = chain_stream.next() => {
											*if option_first_then_fut.is_none() {
												&mut option_first_then_fut
											} else {
												&mut option_newest_then_fut
											} = Some(apply_then(newest_header));
										} else break None,
										if option_first_then_fut.is_some() => let mapped_header = option_first_then_fut.as_mut().unwrap() => {
											// Keep replaceable_then_fut as it is from a newer header than persistent_then_fut
											break Some((mapped_header, (epoch, chain_stream, option_newest_then_fut)))
										},
										if option_newest_then_fut.is_some() => let mapped_header = option_newest_then_fut.as_mut().unwrap() => {
											// Don't keep persistent_then_fut as it is from an older header than replaceable_then_fut
											break Some((mapped_header, (epoch, chain_stream, None)))
										},
									)
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
