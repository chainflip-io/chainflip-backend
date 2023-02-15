use std::pin::Pin;

use cf_chains::dot::RuntimeVersion;
use futures::{stream::unfold, Stream, StreamExt};

use anyhow::Result;
use tracing::error;

/// A stream that ensures the runtime version always produces the first item as the
/// current runtime version, and that doesn't produce duplicate items.
/// This is so we don't need to make the potentially unsafe assumption that the stream always emits
/// the first item.
pub async fn safe_runtime_version_stream<RuntimeVersionStream>(
	current_version: RuntimeVersion,
	runtime_version_stream: RuntimeVersionStream,
) -> Result<Pin<Box<dyn Stream<Item = RuntimeVersion> + Send>>>
where
	RuntimeVersionStream: Stream<Item = Result<RuntimeVersion>> + Send + Unpin + 'static,
{
	struct StreamState<RuntimeVersionStream>
	where
		RuntimeVersionStream: Stream<Item = Result<RuntimeVersion>> + Send + Unpin,
	{
		stream: RuntimeVersionStream,
		version_to_yield: Option<RuntimeVersion>,
		last_version_yielded: Option<RuntimeVersion>,
	}

	let init_state = StreamState {
		stream: runtime_version_stream,
		version_to_yield: Some(current_version),
		last_version_yielded: None,
	};

	Ok(Box::pin(unfold(init_state, move |mut state| async move {
		loop {
			// We have an item we may want to yield
			if let Some(version) = state.version_to_yield.take() {
				if let Some(last_version_yielded) = state.last_version_yielded {
					let spec_version_has_increased =
						version.spec_version > last_version_yielded.spec_version;
					assert!(
						(version.spec_version == last_version_yielded.spec_version &&
							version.transaction_version ==
								last_version_yielded.transaction_version) ||
							(spec_version_has_increased &&
								version.transaction_version >=
									last_version_yielded.transaction_version),
						"If there is no increase in spec version then we expect no increase in transaction version. 
                        The transaction version cannot increase unless the spec version increases."
					);
					if spec_version_has_increased {
						state.last_version_yielded = Some(version);
						break Some((version, state))
					} else {
						// The version we we want to yield is not greater than the last one we
						// yielded
						continue
					}
				} else {
					// if we haven't yet yielded, we yield the first item
					state.last_version_yielded = Some(version);
					break Some((version, state))
				}
			} else {
				// We do not yet have an item we may want to yield, so let's get one
				if let Some(version) = state.stream.next().await {
					state.version_to_yield = Some(match version {
						Ok(version) => version,
						Err(e) => {
							error!("Error pulling version from inner stream, skipping. Error: {e}",);
							continue
						},
					});
				} else {
					break None
				}
			}
		}
	})))
}

#[cfg(test)]
mod tests {
	use futures::stream;
	use utilities::assert_future_panics;

	use super::*;

	#[tokio::test]
	async fn returns_first_version() {
		let first_version = RuntimeVersion { spec_version: 12, transaction_version: 10 };

		let mut runtime_version_stream =
			safe_runtime_version_stream(first_version, stream::iter([])).await.unwrap();

		assert_eq!(runtime_version_stream.next().await, Some(first_version));
		assert!(runtime_version_stream.next().await.is_none());
	}

	// This is a case we expect to happen pretty much every time, as it is the current behaviour of
	// the underlying rpc stream.
	#[tokio::test]
	async fn deduplicates_first_version_if_at_head_of_stream() {
		let first_version = RuntimeVersion { spec_version: 12, transaction_version: 10 };

		let mut runtime_version_stream =
			safe_runtime_version_stream(first_version, stream::iter([Ok(first_version)]))
				.await
				.unwrap();

		assert_eq!(runtime_version_stream.next().await, Some(first_version));
		assert!(runtime_version_stream.next().await.is_none());
	}

	#[tokio::test]
	async fn produces_second_version_if_greater_than_previous() {
		let first_version = RuntimeVersion { spec_version: 12, transaction_version: 10 };
		let second_version = RuntimeVersion {
			spec_version: first_version.spec_version + 1,
			transaction_version: 10,
		};

		let mut runtime_version_stream =
			safe_runtime_version_stream(first_version, stream::iter([Ok(second_version)]))
				.await
				.unwrap();

		assert_eq!(runtime_version_stream.next().await, Some(first_version));
		assert_eq!(runtime_version_stream.next().await, Some(second_version));
		assert!(runtime_version_stream.next().await.is_none());
	}

	#[tokio::test]
	async fn stream_continues_on_error_in_inner_stream() {
		let first_version = RuntimeVersion { spec_version: 12, transaction_version: 10 };
		let second_version = RuntimeVersion {
			spec_version: first_version.spec_version + 1,
			transaction_version: 10,
		};
		let third_version = RuntimeVersion {
			spec_version: second_version.spec_version + 1,
			transaction_version: 10,
		};

		let mut runtime_version_stream = safe_runtime_version_stream(
			first_version,
			stream::iter([Ok(second_version), Err(anyhow::anyhow!("error")), Ok(third_version)]),
		)
		.await
		.unwrap();

		assert_eq!(runtime_version_stream.next().await, Some(first_version));
		assert_eq!(runtime_version_stream.next().await, Some(second_version));
		assert_eq!(runtime_version_stream.next().await, Some(third_version));
		assert!(runtime_version_stream.next().await.is_none());
	}

	#[tokio::test]
	async fn stream_continues_when_duplicate_items() {
		let first_version = RuntimeVersion { spec_version: 12, transaction_version: 10 };
		let second_version = RuntimeVersion {
			spec_version: first_version.spec_version + 1,
			transaction_version: 10,
		};
		let third_version = RuntimeVersion {
			spec_version: second_version.spec_version + 1,
			transaction_version: 10,
		};

		let mut runtime_version_stream = safe_runtime_version_stream(
			first_version,
			stream::iter([
				Ok(first_version),
				Ok(second_version),
				Ok(second_version),
				Ok(third_version),
			]),
		)
		.await
		.unwrap();

		assert_eq!(runtime_version_stream.next().await, Some(first_version));
		assert_eq!(runtime_version_stream.next().await, Some(second_version));
		assert_eq!(runtime_version_stream.next().await, Some(third_version));
		assert!(runtime_version_stream.next().await.is_none());
	}

	#[tokio::test]
	async fn panics_if_invalid_runtime_version_returned() {
		let first_version = RuntimeVersion { spec_version: 12, transaction_version: 10 };

		// The transaction version can not increase without a spec version increase.
		let invalid_second_version = RuntimeVersion {
			spec_version: first_version.spec_version,
			transaction_version: first_version.transaction_version + 1,
		};

		let mut runtime_version_stream =
			safe_runtime_version_stream(first_version, stream::iter([Ok(invalid_second_version)]))
				.await
				.unwrap();

		assert_eq!(runtime_version_stream.next().await, Some(first_version));
		assert_future_panics!(runtime_version_stream.next());
	}
}
