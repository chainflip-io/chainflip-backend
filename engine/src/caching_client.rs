use crate::retrier::TypedFutureGenerator;
use cf_utilities::{task_scope, task_scope::Scope};
use futures_util::FutureExt;
use std::{collections::HashMap, future::Future, hash::Hash, pin::Pin, time::Duration};
use tokio::{
	sync::{mpsc, oneshot},
	time::timeout,
};

const MAX_DURATION: Duration = Duration::new(30, 0);

type PinBoxedFuture<ResultType> =
	Pin<Box<dyn Future<Output = Result<ResultType, anyhow::Error>> + Send>>;
#[derive(Clone)]
pub struct CachingRequest<Key, ResultType, Client> {
	request_sender: mpsc::UnboundedSender<(
		PinBoxedFuture<ResultType>,
		Key,
		oneshot::Sender<Result<ResultType, anyhow::Error>>,
	)>,
	client: Client,
}

impl<
		Key: Eq + Hash + Clone + Send + 'static,
		ResultType: Clone + Send + 'static,
		Client: Clone + Send + Sync + 'static,
	> CachingRequest<Key, ResultType, Client>
{
	pub fn new(scope: &Scope<'_, anyhow::Error>, client: Client) -> (Self, mpsc::Sender<()>) {
		let (request_sender, mut request_receiver) = mpsc::unbounded_channel::<(
			PinBoxedFuture<ResultType>,
			Key,
			oneshot::Sender<Result<ResultType, anyhow::Error>>,
		)>();
		let (cache_invalidation_sender, mut cache_invalidation_receiver) = mpsc::channel::<()>(1);
		scope.spawn_weak({
			task_scope::task_scope(|scope| {
				async move {
					let mut cache: HashMap<Key, ResultType> = HashMap::new();
					let mut in_flight: HashMap<
						Key,
						Vec<oneshot::Sender<Result<ResultType, anyhow::Error>>>,
					> = HashMap::default();
					let (cache_sender, mut cache_receiver) =
						mpsc::unbounded_channel::<(Key, Result<ResultType, anyhow::Error>)>();
					loop {
						tokio::select! {
							biased;
							Some(()) = cache_invalidation_receiver.recv() => {
								// We only clear the cache, we don't want to cancel in_flight requests, this way we are sure that every request will eventually complete
								// regardless of the time it takes to complete
								cache.clear();
							},
							Some((key, value)) = cache_receiver.recv() => {
								if let Ok(ref value) = value {
									cache.insert(key.clone(), value.clone());
								}
								if let Some(senders) = in_flight.remove(&key) {
									for sender in senders {
										let to_send = match &value {
											Ok(val) => Ok(val.clone()),
											Err(e) => Err(anyhow::anyhow!(e.to_string())),
										};
										let _ = sender.send(to_send);
									}
								}
							},
							Some((future, request_key, sender)) = request_receiver.recv() => {
								if let Some(value) = cache.get(&request_key)  {
										let _ = sender.send(Ok(value.clone()));
								} else if let Some(result_senders) = in_flight.get_mut(&request_key) {
									result_senders.push(sender);
								} else {
									let sender_result = cache_sender.clone();
									in_flight.insert(request_key.clone(), vec![sender]);
									scope.spawn(async move {
										let result = future.await;
										let _ = sender_result.send((request_key, result));
										Ok(())
									})
								}

							},
						}
					}
				}
				.boxed()
			})
		});

		(Self { request_sender, client }, cache_invalidation_sender)
	}

	pub(crate) async fn get(
		&self,
		future: TypedFutureGenerator<ResultType, Client>,
		key: Key,
	) -> Result<ResultType, anyhow::Error> {
		let (tx, rx) = oneshot::channel::<Result<ResultType, anyhow::Error>>();
		let client = self.client.clone();
		let future = future(client);
		self.request_sender.send((future, key, tx)).unwrap();
		let result = timeout(MAX_DURATION, rx).await???;
		Ok(result)
	}
}

#[cfg(test)]
mod test {
	use crate::caching_client::CachingRequest;
	use anyhow::Error;
	use cf_utilities::task_scope::{task_scope, Scope};
	use futures_util::FutureExt;
	use rand::random;
	use std::{
		sync::atomic::{AtomicU32, Ordering},
		time::Duration,
	};
	use tokio::time::sleep;

	static A: AtomicU32 = AtomicU32::new(0);

	trait Rpc {
		async fn a(&self) -> anyhow::Result<u32>;
	}
	#[derive(Clone, Default)]
	struct Client {}
	impl Rpc for Client {
		async fn a(&self) -> anyhow::Result<u32> {
			sleep(Duration::new(1, 0)).await;
			A.fetch_add(1, Ordering::Relaxed);
			Ok(random::<u32>())
		}
	}

	#[tokio::test]
	async fn internal_client_called_once() {
		task_scope(|scope: &Scope<'_, Error>| {
			async {
				let client = Client::default();

				let (caching_client, _) =
					CachingRequest::<(), u32, Client>::new(scope, client.clone());

				let result = caching_client
					.get(
						Box::pin(move |client| {
							#[allow(clippy::redundant_async_block)]
							Box::pin(async move { client.a().await })
						}),
						(),
					)
					.await
					.unwrap();
				let result2 = caching_client
					.get(
						Box::pin(move |client| {
							#[allow(clippy::redundant_async_block)]
							Box::pin(async move { client.a().await })
						}),
						(),
					)
					.await
					.unwrap();

				// without cache invalidation the result get cached and a() is called only once
				assert_eq!(result, result2);
				assert_eq!(A.load(Ordering::Relaxed), 1);
				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}

	#[tokio::test]
	async fn cache_invalidation() {
		task_scope(|scope: &Scope<'_, Error>| {
			async {
				let client = Client::default();

				let (caching_client, cache_invalidation_sender) =
					CachingRequest::<(), u32, Client>::new(scope, client.clone());

				let result = caching_client
					.get(
						Box::pin(move |client| {
							#[allow(clippy::redundant_async_block)]
							Box::pin(async move { client.a().await })
						}),
						(),
					)
					.await
					.unwrap();

				cache_invalidation_sender.send(()).await.unwrap();

				let result2 = caching_client
					.get(
						Box::pin(move |client| {
							#[allow(clippy::redundant_async_block)]
							Box::pin(async move { client.a().await })
						}),
						(),
					)
					.await
					.unwrap();

				// cache was invalidated, the two results differ
				assert_ne!(result, result2);
				// a() is called twice since result was not in cache anymore when we call it the
				// second time
				assert_eq!(A.load(Ordering::Relaxed), 2);

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}
}
