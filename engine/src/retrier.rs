//! Generic request retrier.
//!
//! This module provides a generic request retrier. It is used to retry requests
//! that may fail due to network issues or other transient errors.
//! On each request it applies a timeout, such that requests cannot hang.
//! It applies exponential backoff and jitter to the requests if they fail, and will retry them
//! until they succeed.

use std::{
	any::Any,
	collections::{BTreeMap, VecDeque},
	pin::Pin,
	time::Duration,
};

use crate::metrics::{RPC_RETRIER_REQUESTS, RPC_RETRIER_TOTAL_REQUESTS};
use anyhow::Result;
use core::cmp::min;
use futures::Future;
use futures_util::stream::FuturesUnordered;
use rand::Rng;
use std::fmt;
use tokio::sync::{mpsc, oneshot};
use utilities::{task_scope::Scope, UnendingStream};

#[derive(Debug, Clone)]
enum RetryLimit {
	// For requests that should never fail. Failure in these cases is directly or indirectly the
	// fault of the operator. e.g. a faulty Ethereum node.
	NoLimit,

	// Should be set to some small-ish number for requests we expect can fail for a fault
	// other than the operator e.g. broadcasts.
	Limit(Attempt),
}

type TypedFutureGenerator<T, Client> = Pin<
	Box<dyn Fn(Client) -> Pin<Box<dyn Future<Output = Result<T, anyhow::Error>> + Send>> + Send>,
>;

type FutureAnyGenerator<Client> = TypedFutureGenerator<BoxAny, Client>;

// The id per *request* from the external caller. This is not tracking *submissions*.
type RequestId = u64;

type Attempt = u32;

#[derive(Debug, Clone)]
pub struct RequestLog {
	rpc_method: String,
	args: Option<String>,
}

impl RequestLog {
	pub fn new(rpc_method: String, args: Option<String>) -> Self {
		Self { rpc_method, args }
	}
}

impl fmt::Display for RequestLog {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		if let Some(args) = &self.args {
			write!(f, "{}({})", self.rpc_method, args)
		} else {
			write!(f, "{}", self.rpc_method)
		}
	}
}

type SubmissionFutureOutput =
	(RequestId, RequestLog, RetryLimit, Result<BoxAny, (anyhow::Error, Attempt)>);
type SubmissionFuture = Pin<Box<dyn Future<Output = SubmissionFutureOutput> + Send + 'static>>;
type SubmissionFutures = FuturesUnordered<SubmissionFuture>;

type RetryDelays = FuturesUnordered<
	Pin<Box<dyn Future<Output = (RequestId, RequestLog, Attempt, RetryLimit)> + Send + 'static>>,
>;

type BoxAny = Box<dyn Any + Send>;

type RequestPackage<Client> = (oneshot::Sender<BoxAny>, FutureAnyGenerator<Client>);

type RequestSent<Client> =
	(oneshot::Sender<BoxAny>, RequestLog, FutureAnyGenerator<Client>, RetryLimit);

/// Tracks all the retries
pub struct RetrierClient<ClientFut, Client> {
	// The channel to send requests to the client.
	request_sender: mpsc::Sender<RequestSent<Client>>,

	// We need the Client to be Send and Sync
	// but the ClientFut can only be Send (not Sync). We don't need the future to be Sync,
	// so we can just wrap it in this Send and Sync PhantomData to make the type Sync.
	_phantom: PhantomDataSendSync<ClientFut>,
}

pub struct PhantomDataSendSync<T>(core::marker::PhantomData<T>);

impl<T> PhantomDataSendSync<T> {
	pub(crate) fn new() -> Self {
		Self(core::marker::PhantomData)
	}
}

unsafe impl<T> Send for PhantomDataSendSync<T> {}
unsafe impl<T> Sync for PhantomDataSendSync<T> {}

impl<ClientFut, Client: Clone> Clone for RetrierClient<ClientFut, Client> {
	fn clone(&self) -> Self {
		Self { request_sender: self.request_sender.clone(), _phantom: PhantomDataSendSync::new() }
	}
}

#[derive(Default)]
pub struct RequestHolder<Client> {
	last_request_id: RequestId,

	stored_requests: BTreeMap<RequestId, RequestPackage<Client>>,
}

impl<Client> RequestHolder<Client> {
	pub fn new() -> Self {
		Self { last_request_id: 0, stored_requests: BTreeMap::new() }
	}

	// Returns the request id of the new request
	pub fn insert(&mut self, request_id: RequestId, request: RequestPackage<Client>) {
		assert!(self.stored_requests.insert(request_id, request).is_none());
	}

	pub fn next_request_id(&mut self) -> RequestId {
		self.last_request_id += 1;
		self.last_request_id
	}

	pub fn remove(&mut self, request_id: &RequestId) -> Option<RequestPackage<Client>> {
		self.stored_requests.remove(request_id)
	}

	pub fn get(&self, request_id: &RequestId) -> Option<&RequestPackage<Client>> {
		self.stored_requests.get(request_id)
	}
}

// Buffers the number of futures that are currently running. And pushes to the buffer when
// a slot is available on a next() call.
struct SubmissionHolder {
	running_submissions: SubmissionFutures,
	maximum_submissions: u32,
	submissions_buffer: VecDeque<SubmissionFuture>,
}

impl SubmissionHolder {
	pub fn new(maximum_submissions: u32) -> Self {
		Self {
			running_submissions: SubmissionFutures::new(),
			maximum_submissions,
			submissions_buffer: Default::default(),
		}
	}

	pub fn push(&mut self, submission: SubmissionFuture) {
		if (self.running_submissions.len() as u32) < self.maximum_submissions {
			self.running_submissions.push(submission);
		} else {
			self.submissions_buffer.push_back(submission);
		}
	}

	pub async fn next_or_pending(&mut self) -> SubmissionFutureOutput {
		let next_output = self.running_submissions.next_or_pending().await;
		if let Some(buffered_submission) = self.submissions_buffer.pop_front() {
			self.running_submissions.push(buffered_submission);
		}
		next_output
	}
}

const MAX_DELAY_TIME_MILLIS: Duration = Duration::from_secs(10 * 60);

fn max_sleep_duration(initial_request_timeout: Duration, attempt: u32) -> Duration {
	min(MAX_DELAY_TIME_MILLIS, initial_request_timeout.saturating_mul(2u32.saturating_pow(attempt)))
}

// Creates a future of a particular submission.
fn submission_future<Client: Clone>(
	client: Client,
	request_log: RequestLog,
	retry_limit: RetryLimit,
	submission_fn: &FutureAnyGenerator<Client>,
	request_id: RequestId,
	initial_request_timeout: Duration,
	attempt: Attempt,
) -> SubmissionFuture {
	let submission_fut = submission_fn(client);
	// Apply exponential backoff to the request.
	Box::pin(async move {
		(
			request_id,
			request_log.clone(),
			retry_limit,
			match tokio::time::timeout(
				max_sleep_duration(initial_request_timeout, attempt),
				submission_fut,
			)
			.await
			{
				Ok(Ok(t)) => Ok(t),
				Ok(Err(e)) => Err(e),
				Err(_) => Err(anyhow::anyhow!("Request timed out")),
			}
			.map_err(|e| (e, attempt)),
		)
	})
}

/// Requests submitted to this client will be retried until success.
/// When a request fails it will be retried after a delay that exponentially increases on each retry
/// attempt.
impl<ClientFut, Client> RetrierClient<ClientFut, Client>
where
	ClientFut: Future<Output = Client> + Send + 'static,
	Client: Clone + Send + Sync + 'static,
{
	pub fn new(
		scope: &Scope<'_, anyhow::Error>,
		// The name of the retrier that appears in the logs.
		name: &'static str,
		primary_client: ClientFut,
		initial_request_timeout: Duration,
		maximum_concurrent_submissions: u32,
	) -> Self {
		let (request_sender, mut request_receiver) = mpsc::channel::<RequestSent<Client>>(1);

		let mut request_holder = RequestHolder::new();

		let mut retry_delays = RetryDelays::new();

		// This holds any submissions that are waiting for a slot to open up.
		let mut submission_holder = SubmissionHolder::new(maximum_concurrent_submissions);

		scope.spawn(async move {
			let primary_client = primary_client.await;

			utilities::loop_select! {
				if let Some((response_sender, request_log, closure, retry_limit)) = request_receiver.recv() => {
					let request_id = request_holder.next_request_id();
					tracing::debug!("Retrier {name}: Received request `{request_log}` assigning request_id `{request_id}`");
					submission_holder.push(submission_future(primary_client.clone(), request_log, retry_limit, &closure, request_id, initial_request_timeout, 0));
					request_holder.insert(request_id, (response_sender, closure));
				},
				let (request_id, request_log, retry_limit, result) = submission_holder.next_or_pending() => {
					RPC_RETRIER_TOTAL_REQUESTS.with_label_values(&[name, request_log.rpc_method.as_str()]).inc();
					match result {
						Ok(value) => {
							RPC_RETRIER_REQUESTS.with_label_values(&[name, request_log.rpc_method.as_str()]).inc();
							if let Some((response_sender, _)) = request_holder.remove(&request_id) {
								let _result = response_sender.send(value);
							}
						},
						Err((e, attempt)) => {
							// Apply exponential back off with jitter to the retries.
							// We avoid small delays by always having a time of at least half.
							let half_max = max_sleep_duration(initial_request_timeout, attempt) / 2;
							let sleep_duration = half_max + rand::thread_rng().gen_range(Duration::default()..half_max);
							tracing::error!("Retrier {name}: Error for request `{request_log}` with id `{request_id}`, attempt `{attempt}`: {e}. Delaying for {}ms", sleep_duration.as_millis());

							// Delay the request before the next retry.
							retry_delays.push(Box::pin(
								async move {
									tokio::time::sleep(sleep_duration).await;
									(request_id, request_log, attempt, retry_limit)
								}
							));
						},
					}
				},
				let (request_id, request_log, attempt, retry_limit) = retry_delays.next_or_pending() => {
					let next_attempt = attempt.saturating_add(1);

					let (response_sender, closure) = request_holder.get(&request_id).expect("We only remove these on success, and if it's in `retry_delays` then it must still be in `request_holder`");

					if response_sender.is_closed() {
						tracing::trace!("Retrier {name}: Dropped request `{request_log}` with id `{request_id}`. Not retrying.");
						request_holder.remove(&request_id);
					} else {
						match retry_limit {
							RetryLimit::Limit(max_attempts) if next_attempt >= max_attempts => {
								tracing::trace!("Retrier {name}: Has reached maximum attempts of `{max_attempts}` for `{request_log}` with id `{request_id}`. Not retrying.");
								request_holder.remove(&request_id);
							}
							_ => {
								tracing::trace!("Retrier {name}: Retrying request `{request_log}` with id `{request_id}`, attempt `{next_attempt}`");
								submission_holder.push(submission_future(primary_client.clone(), request_log, retry_limit, closure, request_id, initial_request_timeout, next_attempt));
							}
						}
					}
				},
			};
			Ok(())
		});

		Self { request_sender, _phantom: PhantomDataSendSync::new() }
	}

	// Separate function so we can more easily test.
	async fn send_request<T: Send + 'static>(
		&self,
		specific_closure: TypedFutureGenerator<T, Client>,
		request_log: RequestLog,
		retry_limit: RetryLimit,
	) -> oneshot::Receiver<BoxAny> {
		let future_any_fn: FutureAnyGenerator<Client> = Box::pin(move |client| {
			let future = specific_closure(client);
			Box::pin(async move {
				let result = future.await?;
				let result: BoxAny = Box::new(result);
				Ok(result)
			})
		});
		let (tx, rx) = oneshot::channel::<BoxAny>();
		let _result = self.request_sender.send((tx, request_log, future_any_fn, retry_limit)).await;
		rx
	}

	/// Requests something to be retried by the retry client.
	/// Sets retry limit of no limit, since we expect most requests not to fail.
	pub async fn request<T: Send + 'static>(
		&self,
		specific_closure: TypedFutureGenerator<T, Client>,
		request_log: RequestLog,
	) -> T {
		let rx = self.send_request(specific_closure, request_log, RetryLimit::NoLimit).await;
		let result: BoxAny = rx.await.unwrap();
		*result.downcast::<T>().expect("We know we cast the T into an any, and it is a T that we are receiving. Hitting this is a programmer error.")
	}

	/// Requests something to be retried by the retry client, with an explicit retry limit.
	/// Returns an error if the retry limit is reached.
	pub async fn request_with_limit<T: Send + 'static>(
		&self,
		specific_closure: TypedFutureGenerator<T, Client>,
		request_log: RequestLog,
		retry_limit: Attempt,
	) -> Result<T> {
		let rx = self
			.send_request(specific_closure, request_log.clone(), RetryLimit::Limit(retry_limit))
			.await;
		let result: BoxAny = rx.await.map_err(|_| {
			anyhow::anyhow!(
				"Maximum attempt of `{retry_limit}` reached for request `{request_log}`."
			)
		})?;
		Ok(*result.downcast::<T>().expect("We know we cast the T into an any, and it is a T that we are receiving. Hitting this is a programmer error."))
	}
}

#[cfg(test)]
mod tests {
	use std::any::Any;

	use futures_util::FutureExt;
	use tokio::time::timeout;
	use utilities::task_scope::task_scope;

	use super::*;

	fn specific_fut_closure<T: Send + Sync + Clone + 'static, Client>(
		value: T,
		timeout: Duration,
	) -> TypedFutureGenerator<T, Client> {
		Box::pin(move |_client| {
			let value = value.clone();
			Box::pin(async move {
				// We need to delay in the tests, else we'll resolve immediately, meaning the
				// channel is sent down, and can theoretically be replaced using the same request id
				// and the tests will still work despite there potentially being a bug in the
				// implementation.
				tokio::time::sleep(timeout).await;
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
				const INITIAL_TIMEOUT: Duration = Duration::from_millis(100);

				let retrier_client =
					RetrierClient::new(scope, "test", async move { () }, INITIAL_TIMEOUT, 100);

				const REQUEST_1: u32 = 32;
				let rx1 = retrier_client
					.send_request(
						specific_fut_closure(REQUEST_1, INITIAL_TIMEOUT),
						RequestLog::new("request 1".to_string(), None),
						RetryLimit::NoLimit,
					)
					.await;

				const REQUEST_2: u64 = 64;
				let rx2 = retrier_client
					.send_request(
						specific_fut_closure(REQUEST_2, INITIAL_TIMEOUT),
						RequestLog::new("request 2".to_string(), None),
						RetryLimit::NoLimit,
					)
					.await;

				const REQUEST_3: u128 = 128;
				let rx3 = retrier_client
					.send_request(
						specific_fut_closure(REQUEST_3, INITIAL_TIMEOUT),
						RequestLog::new("request 3".to_string(), None),
						RetryLimit::NoLimit,
					)
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
				const TIMEOUT: Duration = Duration::from_millis(1000);
				const INITIAL_TIMEOUT: Duration = Duration::from_millis(50);

				let retrier_client =
					RetrierClient::new(scope, "test", async move { () }, INITIAL_TIMEOUT, 100);

				const REQUEST_1: u32 = 32;
				let rx1 = retrier_client
					.send_request(
						specific_fut_closure(REQUEST_1, TIMEOUT),
						RequestLog::new("request 1".to_string(), None),
						RetryLimit::NoLimit,
					)
					.await;

				const REQUEST_2: u64 = 64;
				let rx2 = retrier_client
					.send_request(
						specific_fut_closure(REQUEST_2, TIMEOUT),
						RequestLog::new("request 2".to_string(), None),
						RetryLimit::NoLimit,
					)
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
				const INITIAL_TIMEOUT: Duration = Duration::from_millis(100);

				let retrier_client =
					RetrierClient::new(scope, "test", async move { () }, INITIAL_TIMEOUT, 100);

				const REQUEST_1: u32 = 32;
				assert_eq!(
					REQUEST_1,
					retrier_client
						.request(
							specific_fut_closure(REQUEST_1, INITIAL_TIMEOUT),
							RequestLog::new("request 1".to_string(), None),
						)
						.await
				);

				const REQUEST_2: u64 = 64;
				assert_eq!(
					REQUEST_2,
					retrier_client
						.request(
							specific_fut_closure(REQUEST_2, INITIAL_TIMEOUT),
							RequestLog::new("request 2".to_string(), None),
						)
						.await
				);

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}

	#[tokio::test]
	async fn using_the_request_with_limit_interface_works() {
		task_scope(|scope| {
			async move {
				const INITIAL_TIMEOUT: Duration = Duration::from_millis(100);

				let retrier_client =
					RetrierClient::new(scope, "test", async move { () }, INITIAL_TIMEOUT, 100);

				const REQUEST_1: u32 = 32;
				assert_eq!(
					REQUEST_1,
					retrier_client
						.request_with_limit(
							specific_fut_closure(REQUEST_1, INITIAL_TIMEOUT),
							RequestLog::new("request 1".to_string(), None),
							5
						)
						.await
						.unwrap()
				);

				const REQUEST_2: u64 = 64;
				assert_eq!(
					REQUEST_2,
					retrier_client
						.request_with_limit(
							specific_fut_closure(REQUEST_2, INITIAL_TIMEOUT),
							RequestLog::new("request 2".to_string(), None),
							5
						)
						.await
						.unwrap()
				);

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}

	#[tokio::test]
	async fn once_at_max_concurrent_submissions_cannot_submit_more_no_limit_requests() {
		task_scope(|scope| {
			async move {
				const TIMEOUT: Duration = Duration::from_millis(200);

				const INITIAL_TIMEOUT: Duration = Duration::from_millis(1000);

				let retrier_client =
					RetrierClient::new(scope, "test", async move { () }, INITIAL_TIMEOUT, 2);

				// Requests 1 and 2 fill the future buffer.
				const REQUEST_1: u32 = 32;
				let rx1 = retrier_client
					.send_request(
						specific_fut_closure(REQUEST_1, TIMEOUT),
						RequestLog::new("request 1".to_string(), None),
						RetryLimit::NoLimit,
					)
					.await;

				const REQUEST_2: u64 = 64;
				let rx2 = retrier_client
					.send_request(
						specific_fut_closure(REQUEST_2, TIMEOUT),
						RequestLog::new("request 2".to_string(), None),
						RetryLimit::NoLimit,
					)
					.await;

				// The submission buffer should be full of the first two requests. We set the
				// timeout here to 0. Such that if the submission buffer is not working, we would
				// expect this request call to resolve immediately.
				const REQUEST_3: u128 = 128;
				timeout(
					Duration::from_millis(100),
					retrier_client.request(
						specific_fut_closure(REQUEST_3, Duration::default()),
						RequestLog::new("request 3".to_string(), None),
					),
				)
				.await
				.unwrap_err();

				// This future will wait for the first two requests to complete.
				assert_eq!(
					timeout(
						Duration::from_millis(600),
						retrier_client.request(
							specific_fut_closure(REQUEST_3, Duration::default()),
							RequestLog::new("request 3".to_string(), None),
						),
					)
					.await
					.unwrap(),
					REQUEST_3,
				);

				check_result(rx1, REQUEST_1).await;
				check_result(rx2, REQUEST_2).await;

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}

	fn specific_fut_err<T: Send + Clone + 'static, Client>(
		timeout: Duration,
	) -> TypedFutureGenerator<T, Client> {
		Box::pin(move |_client| {
			Box::pin(async move {
				tokio::time::sleep(timeout).await;
				Err(anyhow::anyhow!("Sorry, this just doesn't work."))
			})
		})
	}

	#[tokio::test]
	async fn using_the_request_with_limit_fails_after_some_attempts() {
		task_scope(|scope| {
			async move {
				const INITIAL_TIMEOUT: Duration = Duration::from_millis(100);

				let retrier_client =
					RetrierClient::new(scope, "test", async move { () }, INITIAL_TIMEOUT, 100);

				retrier_client
					.request_with_limit(
						specific_fut_err::<(), _>(INITIAL_TIMEOUT),
						RequestLog::new("request".to_string(), None),
						5,
					)
					.await
					.unwrap_err();

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}

	#[tokio::test]
	#[ignore = "Test runs forever. Useful for manually testing the failing requests will never return (because they are retried until success)."]
	async fn request_always_fails() {
		task_scope(|scope| {
			async move {
				const INITIAL_TIMEOUT: Duration = Duration::from_millis(100);

				let retrier_client =
					RetrierClient::new(scope, "test", async move { () }, INITIAL_TIMEOUT, 100);

				retrier_client
					.request(
						specific_fut_err::<(), _>(INITIAL_TIMEOUT),
						RequestLog::new("request".to_string(), None),
					)
					.await;

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}
}
