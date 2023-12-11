use futures_core::{Future, Stream};
use futures_util::{stream, FutureExt, StreamExt};
use std::pin::Pin;
use utilities::loop_select;

use crate::witness::common::{
	chain_source::{aliases, BoxChainStream, ChainStream, Header},
	epoch_source::Epoch,
	BoxActiveAndFuture,
};

use super::{then::ThenClient, ChunkedChainSource};

pub struct LatestThen<Inner, F> {
	inner: Inner,
	f: F,
}
impl<Inner, F> LatestThen<Inner, F> {
	pub fn new(inner: Inner, f: F) -> Self {
		Self { inner, f }
	}
}

/// Applies `then_fn` to the latest available headers in chain_stream (skipping any "backlog"
/// headers), producing a new stream corresponding to the processed items.
fn latest_then_stream<'a, ItemStream, Fut, ThenFn, Output, Index, Hash, Data, Info, HistoricInfo>(
	chain_stream: Pin<Box<ItemStream>>,
	epoch: Epoch<Info, HistoricInfo>,
	then_fn: &'a ThenFn,
) -> BoxChainStream<'a, Index, Hash, Output>
where
	ItemStream: Stream<Item = Header<Index, Hash, Data>> + Send + 'a + ?Sized,
	ThenFn:
		Fn(Epoch<Info, HistoricInfo>, Header<Index, Hash, Data>) -> Fut + Send + Sync + Clone + 'a,
	Fut: Future<Output = Output> + Send,
	Output: Send + Sync + Unpin + 'static,
	Index: aliases::Index,
	Hash: aliases::Hash,
	Data: aliases::Data,
	Info: Clone + Send + Sync + 'static,
	HistoricInfo: Clone + Send + Sync + 'static,
{
	stream::unfold(
			(epoch.clone(), chain_stream, None),
			move |(epoch, mut chain_stream, option_old_then_fut)| {
				async move {
					let apply_then = |header: Header<_, _, _>| {
						let epoch = epoch.clone();
						#[allow(clippy::redundant_async_block)]
						header
							.then_data(move |header| async move { then_fn(epoch, header).await })
							.boxed()
					};

					let (
						// The future for the first header we see
						mut option_first_then_fut,
						// The future for the newest header we've seen
						mut option_newest_then_fut,
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
						}
						.map(apply_then);

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
							// Keep option_newest_then_fut as it is from a newer header than option_first_then_fut
							break Some((mapped_header, (epoch, chain_stream, option_newest_then_fut)))
						},
						if option_newest_then_fut.is_some() => let mapped_header = option_newest_then_fut.as_mut().unwrap() => {
							// Don't keep option_first_then_fut as it is from an older header than option_newest_then_fut
							break Some((mapped_header, (epoch, chain_stream, None)))
						},
					)

				}.boxed()
			},
		)
		.into_box()
}

#[async_trait::async_trait]
impl<Inner: ChunkedChainSource, Output, Fut, F> ChunkedChainSource for LatestThen<Inner, F>
where
	Output: aliases::Data,
	Fut: Future<Output = Output> + Send,
	F: Fn(
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

	type Client = ThenClient<Inner, F>;

	type Chain = Inner::Chain;

	type Parameters = Inner::Parameters;

	async fn stream(
		&self,
		parameters: Self::Parameters,
	) -> BoxActiveAndFuture<'_, super::Item<'_, Self, Self::Info, Self::HistoricInfo>> {
		self.inner
			.stream(parameters)
			.await
			.then(move |(epoch, chain_stream, chain_client)| async move {
				(
					epoch.clone(),
					latest_then_stream(chain_stream, epoch.clone(), &self.f),
					ThenClient::new(chain_client, self.f.clone(), epoch),
				)
			})
			.await
			.into_box()
	}
}

#[cfg(test)]
mod tests {

	use super::*;
	use crate::common::Signal;

	type Index = u64;
	type Hash = ();
	type Data = u32;

	fn to_header(i: u32) -> Header<Index, Hash, Data> {
		Header { index: i as u64, hash: (), parent_hash: None, data: i }
	}

	#[tokio::test]
	async fn test_latest_then_stream() {
		let epoch = Epoch {
			index: 1,
			info: (),
			historic_signal: Signal::<()>::new().1,
			expired_signal: Signal::<()>::new().1,
		};

		let (header_sender, header_receiver) = tokio::sync::mpsc::channel(10);

		let chain_stream = tokio_stream::wrappers::ReceiverStream::new(header_receiver).boxed();

		let then_fn = |epoch, header: Header<_, _, _>| async move { (epoch, header) };
		let mut res_stream = latest_then_stream(chain_stream, epoch.clone(), &then_fn);

		// Initial header isn't initially available, but will be processed when it eventually
		// arrives
		tokio::spawn({
			let header_sender = header_sender.clone();
			async move {
				tokio::time::sleep(std::time::Duration::from_millis(100)).await;
				header_sender.send(to_header(1)).await.unwrap();
			}
		});

		let res = res_stream.next().await.unwrap();
		assert_eq!(res.index, 1);

		// Check that correct epoch/header is provided to the closure:
		assert_eq!(res.data.0.index, epoch.index);
		assert_eq!(res.data.1, to_header(1));

		// Next header arrives as expected:
		header_sender.send(to_header(2)).await.unwrap();
		assert_eq!(res_stream.next().await.unwrap().index, 2);

		// The resulting stream ends when the source ends
		drop(header_sender);
		assert!(res_stream.next().await.is_none());
	}

	#[tokio::test]
	async fn latest_then_skips_older_headers() {
		let epoch = Epoch {
			index: 1,
			info: (),
			historic_signal: Signal::<()>::new().1,
			expired_signal: Signal::<()>::new().1,
		};
		let (header_sender, header_receiver) = tokio::sync::mpsc::channel(10);
		let chain_stream = tokio_stream::wrappers::ReceiverStream::new(header_receiver).boxed();

		let then_fn = |epoch, header: Header<_, _, _>| async move { (epoch, header) };
		let mut res_stream = latest_then_stream(chain_stream, epoch.clone(), &then_fn);

		// Two headers are available immediately, only the last one is processed
		header_sender.send(to_header(1)).await.unwrap();
		header_sender.send(to_header(2)).await.unwrap();
		let res = res_stream.next().await.unwrap();

		assert_eq!(res.index, 2);
	}

	#[tokio::test]
	async fn latest_then_skips_slow_processing_header() {
		let epoch = Epoch {
			index: 1,
			info: (),
			historic_signal: Signal::<()>::new().1,
			expired_signal: Signal::<()>::new().1,
		};

		let (header_sender, header_receiver) = tokio::sync::mpsc::channel(10);
		let chain_stream = tokio_stream::wrappers::ReceiverStream::new(header_receiver).boxed();

		let then_fn = |epoch, header: Header<_, _, _>| async move {
			// First header will take a bit longer to process
			if header.index == 1 {
				tokio::time::sleep(std::time::Duration::from_millis(200)).await;
			}
			(epoch, header)
		};
		let mut res_stream = latest_then_stream(chain_stream, epoch.clone(), &then_fn);

		// Header (1) arrives first, but header (2) arrives shortly after and gets
		// processed before header (1) is processed:
		header_sender.send(to_header(1)).await.unwrap();
		tokio::spawn({
			let header_sender = header_sender.clone();
			async move {
				tokio::time::sleep(std::time::Duration::from_millis(100)).await;
				header_sender.send(to_header(2)).await.unwrap();
			}
		});

		let res = res_stream.next().await.unwrap();

		// Header (1) won't be returned:
		assert_eq!(res.index, 2);
	}
}
