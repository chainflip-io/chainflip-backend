use std::sync::Arc;

use async_trait::async_trait;
use sp_core::H256;
use sp_runtime::traits::Hash;
use tokio::sync::{mpsc, oneshot};
use utilities::task_scope::{Scope, ScopedJoinHandle, OR_CANCEL};

use super::{
	super::{base_rpc_api, SUBSTRATE_BEHAVIOUR},
	common::send_request,
};

// Note 'static on the generics in this trait are only required for mockall to mock it
#[async_trait]
pub trait UnsignedExtrinsicApi {
	async fn submit_unsigned_extrinsic<Call>(&self, call: Call) -> H256
	where
		Call: Into<state_chain_runtime::RuntimeCall>
			+ Clone
			+ std::fmt::Debug
			+ Send
			+ Sync
			+ 'static;
}

pub struct UnsignedExtrinsicClient {
	request_sender: mpsc::Sender<(state_chain_runtime::RuntimeCall, oneshot::Sender<H256>)>,
	_task_handle: ScopedJoinHandle<()>,
}
impl UnsignedExtrinsicClient {
	pub fn new<BaseRpcClient: base_rpc_api::BaseRpcApi + Send + Sync + 'static>(
		scope: &Scope<'_, anyhow::Error>,
		base_rpc_client: Arc<BaseRpcClient>,
	) -> Self {
		const REQUEST_BUFFER: usize = 16;

		let (request_sender, mut request_receiver) = mpsc::channel(REQUEST_BUFFER);

		Self {
			request_sender,
			_task_handle: scope.spawn_with_handle(async move {
				while let Some((call, result_sender)) = request_receiver.recv().await {
					let _result = result_sender.send({
						let extrinsic =
							state_chain_runtime::UncheckedExtrinsic::new_unsigned(call.clone());
						let expected_hash = sp_runtime::traits::BlakeTwo256::hash_of(&extrinsic);
						match base_rpc_client.submit_extrinsic(extrinsic).await {
							Ok(tx_hash) => {
								assert_eq!(tx_hash, expected_hash, "{SUBSTRATE_BEHAVIOUR}");
								tx_hash
							},
							Err(rpc_err) => {
								match rpc_err {
									// POOL_ALREADY_IMPORTED error occurs when the
									// transaction is already in the pool More than one node
									// can submit the same unsigned extrinsic. E.g. in the
									// case of a threshold signature success. Thus, if we
									// get a "Transaction already in pool" "error" we know
									// that this particular extrinsic has already been
									// submitted. And so we can ignore the error and return
									// the transaction hash
									jsonrpsee::core::Error::Call(
										jsonrpsee::types::error::CallError::Custom(ref obj),
									) if obj.code() == 1013 => {
										tracing::debug!(
											"Already in pool with tx_hash: {expected_hash:#x}."
										);
										expected_hash
									},
									// POOL_TEMPORARILY_BANNED error is not entirely understood, we
									// believe it has a similiar meaning to POOL_ALREADY_IMPORTED,
									// but we don't know. We believe there maybe cases where we need
									// to resubmit if this error occurs.
									jsonrpsee::core::Error::Call(
										jsonrpsee::types::error::CallError::Custom(ref obj),
									) if obj.code() == 1012 => {
										tracing::debug!(
											"Transaction is temporarily banned with tx_hash: {expected_hash:#x}."
										);
										expected_hash
									},
									_ => return Err(rpc_err.into()),
								}
							},
						}
					});
				}

				Ok(())
			}),
		}
	}
}

#[async_trait]
impl UnsignedExtrinsicApi for UnsignedExtrinsicClient {
	async fn submit_unsigned_extrinsic<Call>(&self, call: Call) -> H256
	where
		Call: Into<state_chain_runtime::RuntimeCall>
			+ Clone
			+ std::fmt::Debug
			+ Send
			+ Sync
			+ 'static,
	{
		send_request(&self.request_sender, |result_sender| (call.into(), result_sender))
			.await
			.await
			.expect(OR_CANCEL)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::state_chain_observer::client::{
		base_rpc_api::MockBaseRpcApi, extrinsic_api::signed::DUMMY_CALL,
	};
	use futures_util::FutureExt;
	use utilities::task_scope::task_scope;

	#[tokio::test]
	async fn test_successful_unsigned_extrinsic() {
		task_scope(|scope| {
			async {
				let mut mock_rpc_api = MockBaseRpcApi::new();

				// Return a success with the hash of the extrinsic
				mock_rpc_api.expect_submit_extrinsic().once().returning(|extrinsic| {
					Ok(sp_runtime::traits::BlakeTwo256::hash_of(&extrinsic))
				});

				let client = UnsignedExtrinsicClient::new(scope, Arc::new(mock_rpc_api));

				// Send the request
				let (result_sender, result_receiver) = oneshot::channel();
				client.request_sender.send((DUMMY_CALL.clone(), result_sender)).await.unwrap();

				// Should get back the hash of the extrinsic
				let expected_hash = sp_runtime::traits::BlakeTwo256::hash_of(
					&state_chain_runtime::UncheckedExtrinsic::new_unsigned(DUMMY_CALL.clone()),
				);
				assert_eq!(result_receiver.await.unwrap(), expected_hash);

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}

	#[tokio::test]
	async fn should_not_error_if_already_in_pool() {
		task_scope(|scope| {
			async {
				let mut mock_rpc_api = MockBaseRpcApi::new();

				// Return the 1013 error code
				mock_rpc_api.expect_submit_extrinsic().once().returning(move |_| {
					Err(jsonrpsee::core::Error::Call(jsonrpsee::types::error::CallError::Custom(
						jsonrpsee::types::ErrorObject::owned(
							1013,
							"Invalid Transaction",
							Some("POOL_ALREADY_IMPORTED"),
						),
					)))
				});

				let client = UnsignedExtrinsicClient::new(scope, Arc::new(mock_rpc_api));

				// Send the request
				let (result_sender, result_receiver) = oneshot::channel();
				client.request_sender.send((DUMMY_CALL.clone(), result_sender)).await.unwrap();

				// Even with the error, we should still get back the hash of the extrinsic
				let expected_hash = sp_runtime::traits::BlakeTwo256::hash_of(
					&state_chain_runtime::UncheckedExtrinsic::new_unsigned(DUMMY_CALL.clone()),
				);
				assert_eq!(result_receiver.await.unwrap(), expected_hash);

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}

	#[tokio::test]
	async fn should_error_if_unexpected_error() {
		task_scope::<(), anyhow::Error, _>(|scope| {
			async {
				let mut mock_rpc_api = MockBaseRpcApi::new();

				// Return an unexpected error
				mock_rpc_api.expect_submit_extrinsic().once().returning(move |_| {
					Err(jsonrpsee::core::Error::Call(
						jsonrpsee::types::error::CallError::InvalidParams(anyhow::Error::msg(
							"🤷‍♂️",
						)),
					))
				});

				let client = UnsignedExtrinsicClient::new(scope, Arc::new(mock_rpc_api));

				// Send the request
				let (result_sender, result_receiver) = oneshot::channel();
				client.request_sender.send((DUMMY_CALL.clone(), result_sender)).await.unwrap();

				// Should be no result sent back
				assert!(result_receiver.await.is_err());

				// Must return an Ok here so that the `unwrap_err` on the scope will fail if the
				// spawned task is not aborted.
				Ok(())
			}
			.boxed()
		})
		.await
		// The spawned task should aborted, so we expect an error
		.unwrap_err();
	}
}
