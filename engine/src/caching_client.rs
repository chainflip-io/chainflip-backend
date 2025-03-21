use crate::retrier::TypedFutureGenerator;
use cf_utilities::{task_scope, task_scope::Scope};
use futures_util::FutureExt;
use std::{any::Any, collections::HashMap, fmt::Debug, hash::Hash, sync::Arc, time::Duration};
use tokio::{
	sync::{mpsc, oneshot},
	time::timeout,
};

type ArcAny = Arc<dyn Any + Send + Sync>;

type FutureAnyGenerator<Client> = TypedFutureGenerator<ArcAny, Client>;

#[derive(Clone)]
pub struct CachingClient<Key, Client> {
	sender: mpsc::UnboundedSender<(Key, FutureAnyGenerator<Client>, oneshot::Sender<ArcAny>)>,
	pub cache: mpsc::Sender<()>,
}

impl<Key: Eq + Hash + Clone + Send + 'static, Client: Clone + Send + Sync + 'static>
	CachingClient<Key, Client>
{
	pub async fn new(scope: &Scope<'_, anyhow::Error>, client: Client) -> Self {
		let (sender_request, mut receiver_request) =
			mpsc::unbounded_channel::<(Key, FutureAnyGenerator<Client>, oneshot::Sender<ArcAny>)>();
		let (cache_invalidation_sender, mut cache_invalidation_receiver) = mpsc::channel::<()>(1);
		scope.spawn({
			task_scope::task_scope(|scope| {
				async move {
					let mut cache: HashMap<Key, ArcAny> = HashMap::new();
					let mut in_flight: HashMap<Key, Vec<oneshot::Sender<ArcAny>>> =
						HashMap::default();
					let (cache_sender, mut cache_receiver) =
						mpsc::unbounded_channel::<(Key, ArcAny)>();
					loop {
						tokio::select! {
							biased;
							Some(()) = cache_invalidation_receiver.recv() => {
								cache.clear();
							},
							Some((key, value)) = cache_receiver.recv() => {
								if let Some(senders) = in_flight.remove(&key) {
									for sender in senders {
										let _ = sender.send(value.clone());
									}
								}
								cache.insert(key, value);
							},
							Some((request, future_any_fn, sender)) = receiver_request.recv() => {
								if let Some(value) = cache.get(&request)  {
										let _ = sender.send(value.clone());
								} else if let Some(result_senders) = in_flight.get_mut(&request) {
									result_senders.push(sender);
								} else {
									let client = client.clone();
									let sender_result = cache_sender.clone();
									in_flight.insert(request.clone(), vec![sender]);
									scope.spawn(async move {
										let result = future_any_fn(client).await?;
										let _ = sender_result.send((request, result));
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

		Self { sender: sender_request, cache: cache_invalidation_sender }
	}

	pub(crate) async fn get<T: Send + 'static + Clone + Sync + Debug>(
		&self,
		future: TypedFutureGenerator<T, Client>,
		key: Key,
	) -> Result<T, anyhow::Error> {
		let (tx, rx) = oneshot::channel::<ArcAny>();
		let future_any_fn: FutureAnyGenerator<Client> = Box::pin(move |client| {
			let future = future(client);
			Box::pin(async move {
				let result = future.await?;
				let result: ArcAny = Arc::new(result);
				Ok(result)
			})
		});
		self.sender.send((key, future_any_fn, tx)).unwrap();
		let result = timeout(Duration::from_secs(30), rx).await.map_err(anyhow::Error::new)??;
		Ok(result.downcast_ref::<T>().expect("We know we cast the T into an any, and it is a T that we are receiving. Hitting this is a programmer error.").clone())
	}
}
