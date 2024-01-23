use futures_util::StreamExt;
use tokio::sync::oneshot;
use utilities::{
	loop_select, spmc,
	task_scope::{Scope, UnwrapOrCancel},
};

use crate::witness::common::ExternalChainSource;

use super::{BoxChainStream, ChainSource, ChainStream, Header};

type SharedStreamReceiver<InnerSource> = spmc::Receiver<
	Header<
		<InnerSource as ChainSource>::Index,
		<InnerSource as ChainSource>::Hash,
		<InnerSource as ChainSource>::Data,
	>,
>;

type Request<InnerSource> = tokio::sync::oneshot::Sender<(
	SharedStreamReceiver<InnerSource>,
	<InnerSource as ChainSource>::Client,
)>;

#[derive(Clone)]
pub struct SharedSource<InnerSource: ChainSource> {
	request_sender: tokio::sync::mpsc::Sender<Request<InnerSource>>,
}
impl<InnerSource: ChainSource> SharedSource<InnerSource>
where
	InnerSource::Client: Clone,
	InnerSource::Data: Clone,
{
	pub fn new<'a, 'env>(inner_source: InnerSource, scope: &'a Scope<'env, anyhow::Error>) -> Self
	where
		InnerSource: 'env,
	{
		let (request_sender, request_receiver) =
			tokio::sync::mpsc::channel::<Request<InnerSource>>(1);

		scope.spawn(async move {
			let mut request_receiver =
				tokio_stream::wrappers::ReceiverStream::new(request_receiver);

			loop {
				let Some(response_sender) = request_receiver.next().await else { break };

				let (mut inner_stream, inner_client) = inner_source.stream_and_client().await;
				let (mut sender, receiver) = spmc::channel(1);
				let _result = response_sender.send((receiver, inner_client.clone()));

				loop_select!(
					if let Some(response_sender) = request_receiver.next() => {
						let receiver = sender.receiver();
						let _result = response_sender.send((receiver, inner_client.clone()));
					} else disable,
					if let Some(item) = inner_stream.next() => { // This branch failing causes `sender` to be dropped, this causes the proxy/duplicate streams to also end.
						sender.send(item).await;
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
impl<InnerSource: ChainSource> ChainSource for SharedSource<InnerSource>
where
	InnerSource::Client: Clone,
	InnerSource::Data: Clone,
{
	type Index = InnerSource::Index;
	type Hash = InnerSource::Hash;
	type Data = InnerSource::Data;

	type Client = InnerSource::Client;

	async fn stream_and_client(
		&self,
	) -> (BoxChainStream<'_, Self::Index, Self::Hash, Self::Data>, Self::Client) {
		let (sender, receiver) = oneshot::channel();
		{
			let _result = self.request_sender.send(sender).await;
		}
		let (stream, client) = receiver.unwrap_or_cancel().await;
		(stream.into_box(), client)
	}
}

impl<InnerSource: ExternalChainSource> ExternalChainSource for SharedSource<InnerSource>
where
	InnerSource::Client: Clone,
	InnerSource::Data: Clone,
{
	type Chain = InnerSource::Chain;
}
