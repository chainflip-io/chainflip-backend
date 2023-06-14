//! Generic RPC request retrier.
//!
//! This module provides a generic RPC request retrier. It is used to retry RPC requests
//! that may fail due to network issues or other transient errors.
//! It applies exponential backoff to the requests if they fail, and will retry them until they
//! succeed.

use std::{any::Any, collections::BTreeMap, pin::Pin, time::Duration};

use anyhow::Result;
use futures::Future;
use tokio::sync::{mpsc, oneshot};
use utilities::{futures_unordered_wait::FuturesUnorderedWait, task_scope::Scope};

type TypedFutureGenerator<T, RpcClient> = Pin<
	Box<dyn Fn(RpcClient) -> Pin<Box<dyn Future<Output = Result<T, anyhow::Error>> + Send>> + Send>,
>;

type FutureAnyGenerator<RpcClient> = TypedFutureGenerator<BoxAny, RpcClient>;

// The id per *request* from the external caller. This is not tracking *submissions*.
type RequestId = u64;

type RequestFutures = FuturesUnorderedWait<
	Pin<
		Box<
			dyn Future<Output = (RequestId, Result<BoxAny, (anyhow::Error, u64)>)> + Send + 'static,
		>,
	>,
>;

type RetryDelays =
	FuturesUnorderedWait<Pin<Box<dyn Future<Output = (RequestId, u64)> + Send + 'static>>>;

type BoxAny = Box<dyn Any + Send>;

type RequestPackage<RpcClient> = (oneshot::Sender<BoxAny>, FutureAnyGenerator<RpcClient>);

/// Tracks all the retries
pub struct RpcRetrierClient<RpcClient> {
	// The channel to send requests to the client.
	request_sender: mpsc::Sender<RequestPackage<RpcClient>>,
}

#[derive(Default)]
pub struct RequestHolder<RpcClient> {
	last_request_id: RequestId,

	stored_requests: BTreeMap<RequestId, (oneshot::Sender<BoxAny>, FutureAnyGenerator<RpcClient>)>,
}

impl<RpcClient> RequestHolder<RpcClient> {
	pub fn new() -> Self {
		Self { last_request_id: 0, stored_requests: BTreeMap::new() }
	}

	// Returns the request id of the new request
	pub fn insert(&mut self, request_id: RequestId, request: RequestPackage<RpcClient>) {
		assert!(self.stored_requests.insert(request_id, request).is_none());
	}

	pub fn next_request_id(&mut self) -> RequestId {
		self.last_request_id += 1;
		self.last_request_id
	}

	pub fn remove(&mut self, request_id: &RequestId) -> Option<RequestPackage<RpcClient>> {
		self.stored_requests.remove(request_id)
	}

	pub fn get(&self, request_id: &RequestId) -> Option<&RequestPackage<RpcClient>> {
		self.stored_requests.get(request_id)
	}
}

// Creates a future of a particular submission.
fn submission_future<RpcClient: Clone>(
	client: RpcClient,
	submission_fn: &FutureAnyGenerator<RpcClient>,
	request_id: RequestId,
	timeout_millis: u64,
) -> Pin<Box<impl Future<Output = (RequestId, Result<BoxAny, (anyhow::Error, u64)>)>>> {
	let submission_fut = submission_fn(client);
	Box::pin(async move {
		(
			request_id,
			match tokio::time::timeout(Duration::from_millis(timeout_millis), submission_fut).await
			{
				Ok(Ok(t)) => Ok(t),
				Ok(Err(e)) => Err(e),
				Err(_) => Err(anyhow::anyhow!("Request timed out")),
			}
			.map_err(|e| (e.context("Failed to submit request. Timed out."), timeout_millis)),
		)
	})
}

/// Requests submitted to this client will be retried until success.
/// When a request fails it will be retried after a delay that exponentially increases on each retry
/// attempt.
impl<RpcClient: Clone + Send + Sync + 'static> RpcRetrierClient<RpcClient> {
	pub fn new(
		scope: &Scope<'_, anyhow::Error>,
		primary_client: RpcClient,
		initial_request_timeout_millis: u64,
	) -> Self {
		let (request_sender, mut request_receiver) =
			mpsc::channel::<(oneshot::Sender<BoxAny>, FutureAnyGenerator<RpcClient>)>(1);

		let mut request_holder = RequestHolder::new();

		let mut retry_delays = RetryDelays::new();

		let mut running_futures = RequestFutures::new();

		scope.spawn(async move {
			utilities::loop_select! {
				if let Some((response_sender, closure)) = request_receiver.recv() => {
					let request_id = request_holder.next_request_id();
					running_futures.push(submission_future(primary_client.clone(), &closure, request_id, initial_request_timeout_millis));
					request_holder.insert(request_id, (response_sender, closure));
				},
				if let Some((request_id, result)) = running_futures.next() => {
					match result {
						Ok(value) => {
							if let Some((response_sender, _)) = request_holder.remove(&request_id) {
								let _result = response_sender.send(value);
							}
						},
						Err((e, timeout_millis)) => {
							tracing::error!("Error in for request_id {request_id} request: {e}");
							// Delay the request before the next retry.
							retry_delays.push(Box::pin(
								async move {
									tokio::time::sleep(Duration::from_millis(timeout_millis)).await;
									(request_id, timeout_millis)
								}
							));
						},
					}
				},
				if let Some((request_id, timeout_millis)) = retry_delays.next() => {
					tracing::trace!("Retrying request id: {request_id}, that had a timeout of {timeout_millis}");

					if let Some((response_sender, closure)) = request_holder.get(&request_id) {
						// If the receiver has been dropped, we don't need to retry.
						if !response_sender.is_closed() {
							// Apply exponential backoff.
							running_futures.push(submission_future(primary_client.clone(), closure, request_id, timeout_millis.saturating_mul(2)));
						}
					}
				},
			};
			Ok(())
		});

		Self { request_sender }
	}

	// Separate function so we can more easily test.
	async fn send_request<T: Send + Clone + 'static>(
		&self,
		specific_closure: TypedFutureGenerator<T, RpcClient>,
	) -> oneshot::Receiver<BoxAny> {
		let future_any_fn: FutureAnyGenerator<RpcClient> = Box::pin(move |client| {
			let future = specific_closure(client);
			Box::pin(async move {
				let result = future.await?;
				let result: BoxAny = Box::new(result);
				Ok(result)
			})
		});
		let (tx, rx) = oneshot::channel::<BoxAny>();
		let _result = self.request_sender.send((tx, future_any_fn)).await;
		rx
	}

	/// Requests something to be retried by the retry client.
	pub async fn request<T: Send + Clone + 'static>(
		&self,
		specific_closure: TypedFutureGenerator<T, RpcClient>,
	) -> T {
		let rx = self.send_request(specific_closure).await;
		let result: BoxAny = rx.await.unwrap();
		result.downcast_ref::<T>().expect("We know we cast the T into an any, and it is a T that we are receiving. Hitting this is a programmer error.").clone()
	}
}

#[cfg(test)]
mod tests {
	use std::any::Any;

	use futures_util::FutureExt;
	use utilities::task_scope::task_scope;

	use super::*;

	fn specific_fut_closure<T: Send + Sync + Clone + 'static, RpcClient>(
		value: T,
		timeout_millis: u64,
	) -> TypedFutureGenerator<T, RpcClient> {
		Box::pin(move |_client| {
			let value = value.clone();
			Box::pin(async move {
				// We need to delay in the tests, else we'll resolve immediately, meaning the
				// channel is sent down, and can theoretically be replaced using the same request id
				// and the tests will still work despite there potentially being a bug in the
				// implementation.
				tokio::time::sleep(tokio::time::Duration::from_millis(timeout_millis)).await;
				Ok(value)
			})
		})
	}

	async fn check_result<T: PartialEq + std::fmt::Debug + Send + Clone + 'static>(
		result_rx: oneshot::Receiver<BoxAny>,
		expected: T,
	) {
		let result: Box<dyn Any> = result_rx.await.unwrap();
		let downcasted = result.downcast_ref::<T>().unwrap();
		assert_eq!(downcasted, &expected);
	}

	#[tokio::test]
	async fn requests_pulled_in_different_order_works() {
		task_scope(|scope| {
			async move {
				const INITIAL_TIMEOUT_MILLIS: u64 = 100;

				let retrier_client = RpcRetrierClient::new(scope, (), INITIAL_TIMEOUT_MILLIS);

				const REQUEST_1: u32 = 32;
				let rx1 = retrier_client
					.send_request(specific_fut_closure(REQUEST_1, INITIAL_TIMEOUT_MILLIS))
					.await;

				const REQUEST_2: u64 = 64;
				let rx2 = retrier_client
					.send_request(specific_fut_closure(REQUEST_2, INITIAL_TIMEOUT_MILLIS))
					.await;

				const REQUEST_3: u128 = 128;
				let rx3 = retrier_client
					.send_request(specific_fut_closure(REQUEST_3, INITIAL_TIMEOUT_MILLIS))
					.await;

				// Receive items in a different order to sending
				check_result(rx2, REQUEST_2).await;
				check_result(rx1, REQUEST_1).await;
				check_result(rx3, REQUEST_3).await;

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}

	#[tokio::test]
	async fn longer_timeout_ensures_backoff() {
		task_scope(|scope| {
			async move {
				const TIMEOUT_MILLIS: u64 = 1000;
				const INITIAL_TIMEOUT_MILLIS: u64 = 50;

				let retrier_client = RpcRetrierClient::new(scope, (), INITIAL_TIMEOUT_MILLIS);

				const REQUEST_1: u32 = 32;
				let rx1 = retrier_client
					.send_request(specific_fut_closure(REQUEST_1, TIMEOUT_MILLIS))
					.await;

				const REQUEST_2: u64 = 64;
				let rx2 = retrier_client
					.send_request(specific_fut_closure(REQUEST_2, TIMEOUT_MILLIS))
					.await;

				check_result(rx1, REQUEST_1).await;
				check_result(rx2, REQUEST_2).await;

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}

	#[tokio::test]
	async fn using_the_request_interface_works() {
		task_scope(|scope| {
			async move {
				const INITIAL_TIMEOUT_MILLIS: u64 = 100;

				let retrier_client = RpcRetrierClient::new(scope, (), INITIAL_TIMEOUT_MILLIS);

				const REQUEST_1: u32 = 32;
				assert_eq!(
					REQUEST_1,
					retrier_client
						.request(specific_fut_closure(REQUEST_1, INITIAL_TIMEOUT_MILLIS))
						.await
				);

				const REQUEST_2: u64 = 64;
				assert_eq!(
					REQUEST_2,
					retrier_client
						.request(specific_fut_closure(REQUEST_2, INITIAL_TIMEOUT_MILLIS))
						.await
				);

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}

	fn specific_fut_err<T: Send + Clone + 'static, RpcClient>(
		timeout_millis: u64,
	) -> TypedFutureGenerator<T, RpcClient> {
		Box::pin(move |_client| {
			Box::pin(async move {
				tokio::time::sleep(tokio::time::Duration::from_millis(timeout_millis)).await;
				Err(anyhow::anyhow!("Sorry, this just doesn't work."))
			})
		})
	}

	#[tokio::test]
	#[ignore = "Test runs forever. Useful for manually testing the failing requests will never return (because they are retried until success)."]
	async fn request_always_fails() {
		task_scope(|scope| {
			async move {
				const INITIAL_TIMEOUT_MILLIS: u64 = 100;

				let retrier_client = RpcRetrierClient::new(scope, (), INITIAL_TIMEOUT_MILLIS);

				retrier_client.request(specific_fut_err::<(), _>(INITIAL_TIMEOUT_MILLIS)).await;

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}
}
