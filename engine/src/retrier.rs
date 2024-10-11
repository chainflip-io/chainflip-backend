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

use crate::common::Signal;
use anyhow::Result;
use core::cmp::min;
use futures::{Future, FutureExt};
use futures_util::stream::FuturesUnordered;
use rand::Rng;
use std::{
	fmt,
	fmt::{Display, Formatter},
};
use tokio::sync::{mpsc, oneshot};
use utilities::{
	metrics::{RPC_RETRIER_REQUESTS, RPC_RETRIER_TOTAL_REQUESTS},
	task_scope::Scope,
	UnendingStream,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RetryLimit {
	// For requests that should never fail. Failure in these cases is directly or indirectly the
	// fault of the operator. e.g. a faulty Ethereum node.
	NoLimit,

	// Should be set to some small-ish number for requests we expect can fail for a fault
	// other than the operator e.g. broadcasts.
	Limit(Attempt),
}

type TypedFutureGenerator<T, Client> = Pin<
	Box<
		dyn Fn(Client) -> Pin<Box<dyn Future<Output = Result<T, anyhow::Error>> + Send>>
			+ Send
			+ Sync,
	>,
>;

type FutureAnyGenerator<Client> = TypedFutureGenerator<BoxAny, Client>;

// The id per *request* from the external caller. This is not tracking *submissions*.
type RequestId = u64;

pub type Attempt = u32;

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

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum PrimaryOrBackup {
	Primary,
	Backup,
}

impl std::ops::Not for PrimaryOrBackup {
	type Output = Self;

	fn not(self) -> Self::Output {
		match self {
			PrimaryOrBackup::Primary => PrimaryOrBackup::Backup,
			PrimaryOrBackup::Backup => PrimaryOrBackup::Primary,
		}
	}
}

impl Display for PrimaryOrBackup {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		match self {
			PrimaryOrBackup::Primary => write!(f, "Primary"),
			PrimaryOrBackup::Backup => write!(f, "Backup"),
		}
	}
}

type SubmissionFutureOutput =
	(RequestId, RequestLog, RetryLimit, PrimaryOrBackup, Result<BoxAny, (anyhow::Error, Attempt)>);
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
#[derive(Clone)]
pub struct RetrierClient<Client> {
	// The channel to send requests to the client.
	request_sender: mpsc::Sender<RequestSent<Client>>,
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
fn submission_future<Client: Clone + Send + Sync + 'static>(
	client: Client,
	request_log: RequestLog,
	retry_limit: RetryLimit,
	submission_fn: &FutureAnyGenerator<Client>,
	request_id: RequestId,
	initial_request_timeout: Duration,
	attempt: Attempt,
	primary_or_backup: PrimaryOrBackup,
) -> SubmissionFuture {
	let submission_fut = submission_fn(client);
	// Apply exponential backoff to the request.
	Box::pin(async move {
		(
			request_id,
			request_log.clone(),
			retry_limit,
			primary_or_backup,
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

const TRY_PRIMARY_AFTER: Duration = Duration::from_secs(120);

// Pass in two clients, a primary and an optional backup.
// We can then select the client requested if it's ready, otherwise we return the client that's
// ready first.
#[derive(Clone)]
struct ClientSelector<Client: Clone + Send + Sync + 'static> {
	primary_signal: Signal<(Client, PrimaryOrBackup)>,
	backup_signal: Option<Signal<(Client, PrimaryOrBackup)>>,
	// The client to favour for the next request or attempt.
	prefer: PrimaryOrBackup,
	// The time we last tried the primary. If we haven't tried the primary in some time, then we
	// should try it as it could be back online.
	last_failed_primary: Option<tokio::time::Instant>,
}

impl<Client: Send + Sync + Clone + 'static> ClientSelector<Client> {
	/// Create a new client selector. Note that the initation isn't blocking. We should wait for one
	/// client.
	pub fn new<ClientFut: Future<Output = Client> + Send + 'static>(
		scope: &Scope<'_, anyhow::Error>,
		primary_fut: ClientFut,
		backup_fut: Option<ClientFut>,
	) -> Self {
		let (primary_signaller, primary_signal) = Signal::new();

		scope.spawn_weak(async move {
			let client = primary_fut.await;
			primary_signaller.signal((client, PrimaryOrBackup::Primary));
			Ok(())
		});

		let backup_signal = if let Some(backup_fut) = backup_fut {
			let (backup_signaller, backup_signal) = Signal::new();

			scope.spawn_weak(async move {
				let client = backup_fut.await;
				backup_signaller.signal((client, PrimaryOrBackup::Backup));
				Ok(())
			});
			Some(backup_signal)
		} else {
			None
		};

		Self {
			primary_signal,
			backup_signal,
			prefer: PrimaryOrBackup::Primary,
			last_failed_primary: None,
		}
	}

	// Returns a client, and the type of client selected.
	pub async fn select_client(
		&mut self,
		// We use retry limit to determine if we want to wait or not. The assumption is that if we
		// are willing to stop retrying for a request, then we are also willing to exit early when
		// not ready - to allow the network to select another participant more quickly.
		retry_limit: RetryLimit,
	) -> Option<(Client, PrimaryOrBackup)> {
		let client_select_fut = futures::future::select_all(
			utilities::conditional::conditional(
				&self.backup_signal,
				|backup_signal| {
					// If we have two clients, then we should bias the requested one, but if it's
					// not ready, request from the other one.
					match self.prefer {
						PrimaryOrBackup::Backup => match self.last_failed_primary {
							// If we haven't tried the primary in some time, then we should try it
							// as it could be back online
							Some(last_failed_primary)
								if last_failed_primary.elapsed() > TRY_PRIMARY_AFTER =>
								[&self.primary_signal, backup_signal],

							_ => [backup_signal, &self.primary_signal],
						},
						PrimaryOrBackup::Primary => [&self.primary_signal, backup_signal],
					}
					.into_iter()
				},
				// If we only have a primary, we have to wait for it to be ready.
				|()| [&self.primary_signal].into_iter(),
			)
			.map(|signal| Box::pin(signal.clone().wait())),
		);

		if retry_limit == RetryLimit::NoLimit {
			Some(client_select_fut.await.0)
		} else {
			client_select_fut.now_or_never().map(|(client, ..)| client)
		}
	}

	pub fn request_failed(&mut self, failed_client: PrimaryOrBackup) {
		// If we have a backup endpoint, then we should switch to the other one.
		if self.backup_signal.is_some() {
			self.prefer = if failed_client == PrimaryOrBackup::Primary {
				self.last_failed_primary = Some(tokio::time::Instant::now());
				PrimaryOrBackup::Backup
			} else {
				PrimaryOrBackup::Primary
			};
		}
	}
}

#[async_trait::async_trait]
pub trait RetryLimitReturn: Send + 'static {
	type ReturnType<T>;

	fn into_retry_limit(param_type: Self) -> RetryLimit;

	fn inner_to_return_type<T: Send + 'static>(
		inner: Result<BoxAny, tokio::sync::oneshot::error::RecvError>,
		log_message: String,
	) -> Self::ReturnType<T>;
}

pub struct NoRetryLimit;

impl RetryLimitReturn for NoRetryLimit {
	type ReturnType<T> = T;

	fn into_retry_limit(_param_type: Self) -> RetryLimit {
		RetryLimit::NoLimit
	}

	fn inner_to_return_type<T: Send + 'static>(
		inner: Result<BoxAny, tokio::sync::oneshot::error::RecvError>,
		_log_message: String,
	) -> Self::ReturnType<T> {
		let result: BoxAny = inner.unwrap();
		*result.downcast::<T>().expect("We know we cast the T into an any, and it is a T that we are receiving. Hitting this is a programmer error.")
	}
}

impl RetryLimitReturn for u32 {
	type ReturnType<T> = Result<T>;

	fn into_retry_limit(param_type: Self) -> RetryLimit {
		RetryLimit::Limit(param_type)
	}

	fn inner_to_return_type<T: Send + 'static>(
		inner: Result<BoxAny, tokio::sync::oneshot::error::RecvError>,
		log_message: String,
	) -> Self::ReturnType<T> {
		let result: BoxAny = inner.map_err(|_| anyhow::anyhow!("{log_message}"))?;
		Ok(*result.downcast::<T>().expect("We know we cast the T into an any, and it is a T that we are receiving. Hitting this is a programmer error."))
	}
}

/// Requests submitted to this client will be retried until success.
/// When a request fails it will be retried after a delay that exponentially increases on each retry
/// attempt.
impl<Client> RetrierClient<Client>
where
	Client: Clone + Send + Sync + 'static,
{
	pub fn new<ClientFut: Future<Output = Client> + Send + 'static>(
		scope: &Scope<'_, anyhow::Error>,
		// The name of the retrier that appears in the logs.
		name: &'static str,
		primary_client_fut: ClientFut,
		backup_client_fut: Option<ClientFut>,
		initial_request_timeout: Duration,
		maximum_concurrent_submissions: u32,
	) -> Self {
		let (request_sender, mut request_receiver) = mpsc::channel::<RequestSent<Client>>(1);

		let mut request_holder = RequestHolder::new();

		let mut retry_delays = RetryDelays::new();

		// This holds any submissions that are waiting for a slot to open up.
		let mut submission_holder = SubmissionHolder::new(maximum_concurrent_submissions);

		let mut client_selector: ClientSelector<Client> =
			ClientSelector::new(scope, primary_client_fut, backup_client_fut);

		scope.spawn(async move {
			utilities::loop_select! {
				if let Some((response_sender, request_log, closure, retry_limit)) = request_receiver.recv() => {
					RPC_RETRIER_REQUESTS.inc(&[name, request_log.rpc_method.as_str()]);
					let request_id = request_holder.next_request_id();

					if let Some((client, primary_or_backup)) = client_selector.select_client(retry_limit).await {
						tracing::debug!("Retrier {name}: Received request `{request_log}` assigning request_id `{request_id}` and requesting with `{primary_or_backup:?}`");
						submission_holder.push(submission_future(client, request_log, retry_limit, &closure, request_id, initial_request_timeout, 0, primary_or_backup));
						request_holder.insert(request_id, (response_sender, closure));
					} else {
						tracing::warn!("Retrier {name}: No clients available for request when received `{request_log}` with id `{request_id}`. Dropping request.");
					}
				},
				let (request_id, request_log, retry_limit, primary_or_backup, result) = submission_holder.next_or_pending() => {
					RPC_RETRIER_TOTAL_REQUESTS.inc(&[name, request_log.rpc_method.as_str(), primary_or_backup.to_string().as_str()]);
					match result {
						Ok(value) => {
							if let Some((response_sender, _)) = request_holder.remove(&request_id) {
								let _result = response_sender.send(value);
							}
						},
						Err((e, attempt)) => {
							// Apply exponential back off with jitter to the retries.
							// We avoid small delays by always having a time of at least half.
							let half_max = max_sleep_duration(initial_request_timeout, attempt) / 2;
							let sleep_duration = half_max + rand::thread_rng().gen_range(Duration::default()..half_max);

							let error_message = format!("Retrier {name}: Error for request `{request_log}` with id `{request_id}` requested with `{primary_or_backup:?}`, attempt `{attempt}`: {e}. Delaying for {:?}", sleep_duration);
							if attempt == 0 && !matches!(retry_limit, RetryLimit::Limit(1)) {
								tracing::debug!(error_message);
							} else {
								tracing::warn!(error_message);
							}

							client_selector.request_failed(primary_or_backup);

							// Delay the request before the next retry.
							retry_delays.push(Box::pin(
								async move {
									tokio::time::sleep(sleep_duration).await;
									// pass in primary or backup so we know which client to use.
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
								// This await should always return immediately since we must already have a client if we've already made a request.
								if let Some((next_client, next_primary_or_backup)) = client_selector.select_client(retry_limit).await {
									tracing::trace!("Retrier {name}: Retrying request `{request_log}` with id `{request_id}` and client `{next_primary_or_backup:?}`, attempt `{next_attempt}`");
									submission_holder.push(submission_future(next_client, request_log, retry_limit, closure, request_id, initial_request_timeout, next_attempt, next_primary_or_backup));
								} else {
									tracing::warn!("Retrier {name}: No clients available for request `{request_log}` with id `{request_id}`. Dropping request.");
									request_holder.remove(&request_id);
								}
							}
						}
					}
				},
			};
			Ok(())
		});

		Self { request_sender }
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
		request_log: RequestLog,
		specific_closure: TypedFutureGenerator<T, Client>,
	) -> T {
		self.request_with_limit::<T, NoRetryLimit>(request_log, specific_closure, NoRetryLimit)
			.await
	}

	/// Requests something to be retried by the retry client, with an explicit retry limit.
	/// Returns an error if the retry limit is reached.
	pub async fn request_with_limit<T: Send + 'static, R: RetryLimitReturn>(
		&self,
		request_log: RequestLog,
		specific_closure: TypedFutureGenerator<T, Client>,
		retry_limit: R,
	) -> R::ReturnType<T> {
		let retry_limit = R::into_retry_limit(retry_limit);
		let rx = self.send_request(specific_closure, request_log.clone(), retry_limit).await;
		R::inner_to_return_type(
			rx.await,
			format!("Maximum attempt of `{retry_limit:?}` reached for request `{request_log}`."),
		)
	}
}

#[cfg(test)]
mod tests {
	use std::any::Any;

	use fmt::Debug;
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
					RetrierClient::new(scope, "test", async move {}, None, INITIAL_TIMEOUT, 100);

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
					RetrierClient::new(scope, "test", async move {}, None, INITIAL_TIMEOUT, 100);

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
					RetrierClient::new(scope, "test", async move {}, None, INITIAL_TIMEOUT, 100);

				const REQUEST_1: u32 = 32;
				assert_eq!(
					REQUEST_1,
					retrier_client
						.request(
							RequestLog::new("request 1".to_string(), None),
							specific_fut_closure(REQUEST_1, INITIAL_TIMEOUT),
						)
						.await
				);

				const REQUEST_2: u64 = 64;
				assert_eq!(
					REQUEST_2,
					retrier_client
						.request(
							RequestLog::new("request 2".to_string(), None),
							specific_fut_closure(REQUEST_2, INITIAL_TIMEOUT),
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
					RetrierClient::new(scope, "test", async move {}, None, INITIAL_TIMEOUT, 100);

				const REQUEST_1: u32 = 32;
				assert_eq!(
					REQUEST_1,
					retrier_client
						.request_with_limit(
							RequestLog::new("request 1".to_string(), None),
							specific_fut_closure(REQUEST_1, INITIAL_TIMEOUT),
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
							RequestLog::new("request 2".to_string(), None),
							specific_fut_closure(REQUEST_2, INITIAL_TIMEOUT),
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
					RetrierClient::new(scope, "test", async move {}, None, INITIAL_TIMEOUT, 2);

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
						RequestLog::new("request 3".to_string(), None),
						specific_fut_closure(REQUEST_3, Duration::default()),
					),
				)
				.await
				.unwrap_err();

				// This future will wait for the first two requests to complete.
				assert_eq!(
					timeout(
						Duration::from_millis(600),
						retrier_client.request(
							RequestLog::new("request 3".to_string(), None),
							specific_fut_closure(REQUEST_3, Duration::default()),
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

	fn specific_fut_err<T: Send + Clone + 'static, Client: Debug>(
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
					RetrierClient::new(scope, "test", async move {}, None, INITIAL_TIMEOUT, 100);

				retrier_client
					.request_with_limit(
						RequestLog::new("request".to_string(), None),
						specific_fut_err::<(), _>(INITIAL_TIMEOUT),
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

	async fn get_client(ready: bool) {
		if !ready {
			futures::future::pending().await
		}
	}

	#[tokio::test]
	async fn backup_rpc_succeeds_if_primary_not_ready() {
		task_scope(|scope| {
			async move {
				const INITIAL_TIMEOUT: Duration = Duration::from_millis(100);

				let retrier_client = RetrierClient::new(
					scope,
					"test",
					get_client(false),
					Some(get_client(true)),
					INITIAL_TIMEOUT,
					100,
				);

				const REQUEST_1: u32 = 32;
				assert_eq!(
					REQUEST_1,
					retrier_client
						.request(
							RequestLog::new("request 1".to_string(), None),
							specific_fut_closure(REQUEST_1, INITIAL_TIMEOUT),
						)
						.await
				);

				const REQUEST_2: u64 = 64;
				assert_eq!(
					REQUEST_2,
					retrier_client
						.request(
							RequestLog::new("request 2".to_string(), None),
							specific_fut_closure(REQUEST_2, INITIAL_TIMEOUT),
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
	async fn backup_used_for_next_request_if_primary_fails() {
		async fn get_client_primary_or_backup(
			primary_or_backup: PrimaryOrBackup,
		) -> PrimaryOrBackup {
			primary_or_backup
		}

		thread_local! {
			pub static ATTEMPTED: std::cell::RefCell<u32> = std::cell::RefCell::new(0);
			pub static TRIED_CLIENTS: std::cell::RefCell<Vec<PrimaryOrBackup >> = std::cell::RefCell::new(Vec::new());
		}

		fn specific_fut_closure_err_after_one(
			timeout: Duration,
		) -> TypedFutureGenerator<(), PrimaryOrBackup> {
			Box::pin(move |client| {
				Box::pin(async move {
					// We need to delay in the tests, else we'll resolve immediately, meaning the
					// channel is sent down, and can theoretically be replaced using the same
					// request id and the tests will still work despite there potentially being a
					// bug in the implementation.
					tokio::time::sleep(timeout).await;

					let attempts = ATTEMPTED.with(|cell| *cell.borrow());

					let return_val = if attempts == 0 || attempts == 6 {
						Err(anyhow::anyhow!("Sorry, this just doesn't work."))
					} else {
						Ok(())
					};

					// Update attempt after attempted
					ATTEMPTED.with(|cell| {
						let mut attempted = cell.borrow_mut();
						*attempted += 1;
					});

					TRIED_CLIENTS.with(|cell| {
						let mut tried_clients = cell.borrow_mut();
						tried_clients.push(client);
					});

					return_val
				})
			})
		}

		// === TEST ===

		task_scope(|scope| {
			async move {
				const INITIAL_TIMEOUT: Duration = Duration::from_millis(300);

				let retrier_client = RetrierClient::new(
					scope,
					"test",
					get_client_primary_or_backup(PrimaryOrBackup::Primary),
					Some(get_client_primary_or_backup(PrimaryOrBackup::Backup)),
					INITIAL_TIMEOUT,
					100,
				);

				// The first request will fail, and the second request will succeed using the
				// backup. Then the next two requests will use the backup.
				for request in 0..=3 {
					retrier_client
						.request_with_limit(
							RequestLog::new(request.to_string(), None),
							specific_fut_closure_err_after_one(INITIAL_TIMEOUT),
							5,
						)
						.await
						.unwrap();
				}

				// We want to advance time so that we can try the primary again.
				tokio::time::pause();
				tokio::time::advance(TRY_PRIMARY_AFTER * 2).await;
				tokio::time::resume();

				for request in 4..6 {
					retrier_client
						.request_with_limit(
							RequestLog::new(request.to_string(), None),
							specific_fut_closure_err_after_one(INITIAL_TIMEOUT),
							5,
						)
						.await
						.unwrap();
				}

				// We want to advance time so that we can try the primary again.
				tokio::time::pause();
				tokio::time::advance(TRY_PRIMARY_AFTER * 2).await;
				tokio::time::resume();

				retrier_client
					.request_with_limit(
						RequestLog::new(7.to_string(), None),
						specific_fut_closure_err_after_one(INITIAL_TIMEOUT),
						5,
					)
					.await
					.unwrap();

				// Assert the tried clients is what we expect:
				assert_eq!(
					TRIED_CLIENTS.with(|cell| cell.borrow().clone()),
					vec![
						// first request fails
						PrimaryOrBackup::Primary,
						// first request succeeds on second attempt
						PrimaryOrBackup::Backup,
						// second succeeds
						PrimaryOrBackup::Backup,
						// third succeeds
						PrimaryOrBackup::Backup,
						// fourth succeeds
						PrimaryOrBackup::Backup,
						// try primary again, and succeeds, so no further items in this list
						PrimaryOrBackup::Primary,
						// primary should still be favoured after it succeeds
						PrimaryOrBackup::Primary,
						// primary fails again, so seocondary is used
						PrimaryOrBackup::Backup,
						// time elapsed, primary again
						PrimaryOrBackup::Primary,
					]
				);

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}

	// When we startup the clients may be initialising. If both clients are initialising than we
	// want to exit rather than waiting forever, in the case of requests with a retry limit set such
	// as broadcasts.
	#[tokio::test]
	async fn return_error_when_retry_limit_if_no_client_ready() {
		task_scope(|scope| {
			async move {
				const INITIAL_TIMEOUT: Duration = Duration::from_millis(100);

				let retrier_client = RetrierClient::new(
					scope,
					"test",
					futures::future::pending::<()>(),
					Some(futures::future::pending::<()>()),
					INITIAL_TIMEOUT,
					100,
				);

				retrier_client
					.request_with_limit(
						RequestLog::new("request".to_string(), None),
						// The clients aren't ready - so this future will never actually run.
						specific_fut_err::<(), _>(INITIAL_TIMEOUT),
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
					RetrierClient::new(scope, "test", async move {}, None, INITIAL_TIMEOUT, 100);

				retrier_client
					.request(
						RequestLog::new("request".to_string(), None),
						specific_fut_err::<(), _>(INITIAL_TIMEOUT),
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
