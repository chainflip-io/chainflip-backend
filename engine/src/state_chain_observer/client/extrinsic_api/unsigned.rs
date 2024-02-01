use std::sync::Arc;

use async_trait::async_trait;
use sp_core::H256;
use sp_runtime::{traits::Hash, transaction_validity::InvalidTransaction};
use tokio::sync::{mpsc, oneshot};
use utilities::task_scope::{Scope, ScopedJoinHandle, UnwrapOrCancel};

use crate::state_chain_observer::client::extrinsic_api::common::invalid_err_obj;

use super::{
	super::{base_rpc_api, SUBSTRATE_BEHAVIOUR},
	common::send_request,
};

pub enum ExtrinsicError {
	Stale,
}

// Note 'static on the generics in this trait are only required for mockall to mock it
#[async_trait]
pub trait UnsignedExtrinsicApi {
	async fn submit_unsigned_extrinsic<Call>(&self, call: Call) -> Result<H256, ExtrinsicError>
	where
		Call: Into<state_chain_runtime::RuntimeCall>
			+ Clone
			+ std::fmt::Debug
			+ Send
			+ Sync
			+ 'static;
}

pub struct UnsignedExtrinsicClient {
	request_sender: mpsc::Sender<(
		state_chain_runtime::RuntimeCall,
		oneshot::Sender<Result<H256, ExtrinsicError>>,
	)>,
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
								Ok(tx_hash)
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
										Ok(expected_hash)
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
										Ok(expected_hash)
									},
									jsonrpsee::core::Error::Call(
										jsonrpsee::types::error::CallError::Custom(ref obj),
									) if obj == &invalid_err_obj(InvalidTransaction::Stale) => {
										tracing::debug!("Submission failed as the transaction is stale: {obj:?}");
										Err(ExtrinsicError::Stale)
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
	async fn submit_unsigned_extrinsic<Call>(&self, call: Call) -> Result<H256, ExtrinsicError>
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
			.unwrap_or_cancel()
			.await
	}
}
