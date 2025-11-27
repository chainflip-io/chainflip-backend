use crate::retrier::TypedFutureGenerator;
use cf_utilities::{task_scope, task_scope::Scope};
use futures_util::FutureExt;
use std::{collections::HashMap, future::Future, hash::Hash, pin::Pin, time::Duration};
use tokio::{
	sync::{mpsc, oneshot},
	time::timeout,
};

const MAX_WAIT_TIME_FOR_REQUEST: Duration = Duration::from_secs(6);

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
										let _ = sender.send(match &value {
											Ok(val) => Ok(val.clone()),
											Err(e) => Err(anyhow::anyhow!(e.to_string())),
										});
									}
								}
							},
							Some((future, request_key, result_to_caller_sender)) = request_receiver.recv() => {
								if let Some(value) = cache.get(&request_key)  {
										let _ = result_to_caller_sender.send(Ok(value.clone()));
								} else if let Some(result_senders) = in_flight.get_mut(&request_key) {
									result_senders.push(result_to_caller_sender);
								} else {
									let cache_sender = cache_sender.clone();
									in_flight.insert(request_key.clone(), vec![result_to_caller_sender]);
									scope.spawn(async move {
										let _ = cache_sender.send((request_key, future.await));
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

	pub(crate) async fn get_or_fetch(
		&self,
		future: TypedFutureGenerator<ResultType, Client>,
		key: Key,
	) -> Result<ResultType, anyhow::Error> {
		let (result_sender, result_receiver) =
			oneshot::channel::<Result<ResultType, anyhow::Error>>();
		let client = self.client.clone();
		let future = future(client);
		self.request_sender.send((future, key, result_sender)).expect(
			"Inner loop containing the receiver should never exits, engine is shutting down",
		);
		timeout(MAX_WAIT_TIME_FOR_REQUEST, result_receiver).await??
	}
}

#[cfg(test)]
mod test {
	use crate::caching_request::CachingRequest;
	use anyhow::Error;
	use cf_utilities::task_scope::{task_scope, Scope};
	use futures_util::FutureExt;
	use rand::random;
	use std::{
		sync::atomic::{AtomicU32, Ordering},
		time::Duration,
	};
	use tokio::{time::sleep, try_join};

	static A: AtomicU32 = AtomicU32::new(0);
	static B: AtomicU32 = AtomicU32::new(0);
	static C: AtomicU32 = AtomicU32::new(0);

	trait Rpc {
		async fn request(&self, counter: &AtomicU32) -> anyhow::Result<u32>;
	}
	#[derive(Clone, Default)]
	struct Client {}
	impl Rpc for Client {
		async fn request(&self, counter: &AtomicU32) -> anyhow::Result<u32> {
			counter.fetch_add(1, Ordering::Relaxed);
			sleep(Duration::new(2, 0)).await;
			Ok(random::<u32>())
		}
	}

	#[tokio::test]
	async fn result_in_cache_internal_client_called_once() {
		task_scope(|scope: &Scope<'_, Error>| {
			async {
				let client = Client::default();

				let (caching_client, _) =
					CachingRequest::<(), u32, Client>::new(scope, client.clone());

				let result1 = caching_client
					.get_or_fetch(
						Box::pin(move |client| Box::pin(async move { client.request(&A).await })),
						(),
					)
					.await
					.unwrap();
				// After the first request completes, result is stored in cache
				let result2 = caching_client
					.get_or_fetch(
						Box::pin(move |client| Box::pin(async move { client.request(&A).await })),
						(),
					)
					.await
					.unwrap();

				// without cache invalidation the result get cached and a() is called only once
				assert_eq!(result1, result2);
				assert_eq!(A.load(Ordering::Relaxed), 1);
				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}

	#[tokio::test]
	async fn request_in_flight_internal_client_called_once() {
		task_scope(|scope: &Scope<'_, Error>| {
			async {
				let client = Client::default();

				let (caching_client, _) =
					CachingRequest::<(), u32, Client>::new(scope, client.clone());

				let future1 = caching_client.get_or_fetch(
					Box::pin(move |client| Box::pin(async move { client.request(&B).await })),
					(),
				);
				let future2 = caching_client.get_or_fetch(
					Box::pin(move |client| Box::pin(async move { client.request(&B).await })),
					(),
				);
				// we will start both request simultaneously such that the request is still in
				// flight
				let (result1, result2) = try_join!(future1, future2)?;

				assert_eq!(result1, result2);
				assert_eq!(B.load(Ordering::Relaxed), 1);
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
					.get_or_fetch(
						Box::pin(move |client| Box::pin(async move { client.request(&C).await })),
						(),
					)
					.await
					.unwrap();

				cache_invalidation_sender.send(()).await.unwrap();

				let result2 = caching_client
					.get_or_fetch(
						Box::pin(move |client| Box::pin(async move { client.request(&C).await })),
						(),
					)
					.await
					.unwrap();

				// cache was invalidated, the two results differ
				assert_ne!(result, result2);
				// a() is called twice since result was not in cache anymore when we call it the
				// second time
				assert_eq!(C.load(Ordering::Relaxed), 2);

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}
}
