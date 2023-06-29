use futures_util::StreamExt;
use tokio::sync::oneshot;
use utilities::{
	loop_select,
	task_scope::{Scope, OR_CANCEL},
	UnendingStream,
};

use crate::common::spmc;

use super::{box_chain_stream, BoxChainStream, ChainSourceWithClient, Header};

type SharedStreamReceiver<UnderlyingSource> = spmc::Receiver<
	Header<
		<UnderlyingSource as ChainSourceWithClient>::Index,
		<UnderlyingSource as ChainSourceWithClient>::Hash,
		<UnderlyingSource as ChainSourceWithClient>::Data,
	>,
>;

type Request<UnderlyingSource> = tokio::sync::oneshot::Sender<(
	SharedStreamReceiver<UnderlyingSource>,
	<UnderlyingSource as ChainSourceWithClient>::Client,
)>;

#[derive(Clone)]
pub struct SharedSource<UnderlyingSource: ChainSourceWithClient> {
	request_sender: tokio::sync::mpsc::Sender<Request<UnderlyingSource>>,
}
impl<UnderlyingSource: ChainSourceWithClient> SharedSource<UnderlyingSource>
where
	UnderlyingSource::Client: Clone,
	UnderlyingSource::Data: Clone,
{
	pub fn new<'a, 'env>(
		scope: &'a Scope<'env, anyhow::Error>,
		underlying_source: UnderlyingSource,
	) -> Self
	where
		UnderlyingSource: 'env,
	{
		let (request_sender, request_receiver) =
			tokio::sync::mpsc::channel::<Request<UnderlyingSource>>(1);

		scope.spawn(async move {
			let mut request_receiver =
				tokio_stream::wrappers::ReceiverStream::new(request_receiver);

			loop {
				let Some(response_sender) = request_receiver.next().await else {
					break
				};

				let (mut underlying_stream, underlying_client) =
					underlying_source.stream_and_client().await;
				let (mut sender, receiver) = spmc::channel(1);
				let _result = response_sender.send((receiver, underlying_client.clone()));

				loop_select!(
					if let Some(response_sender) = request_receiver.next() => {
						let receiver = sender.receiver();
						let _result = response_sender.send((receiver, underlying_client.clone()));
					},
					let item = underlying_stream.next_or_pending() => {
						let _result = sender.send(item).await;
					},
					let _ = sender.closed() => { break },
				)
			}
			Ok(())
		});

		Self { request_sender }
	}
}

#[async_trait::async_trait]
impl<UnderlyingSource: ChainSourceWithClient> ChainSourceWithClient
	for SharedSource<UnderlyingSource>
where
	UnderlyingSource::Client: Clone,
	UnderlyingSource::Data: Clone,
{
	type Index = UnderlyingSource::Index;
	type Hash = UnderlyingSource::Hash;
	type Data = UnderlyingSource::Data;

	type Client = UnderlyingSource::Client;

	async fn stream_and_client(
		&self,
	) -> (BoxChainStream<'_, Self::Index, Self::Hash, Self::Data>, Self::Client) {
		let (sender, receiver) = oneshot::channel();
		{
			let _result = self.request_sender.send(sender).await;
		}
		let (stream, client) = receiver.await.expect(OR_CANCEL);
		(box_chain_stream(stream), client)
	}
}
