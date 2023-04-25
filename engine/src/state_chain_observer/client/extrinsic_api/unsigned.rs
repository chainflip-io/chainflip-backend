use std::sync::Arc;

use async_trait::async_trait;
use sp_core::H256;
use sp_runtime::traits::Hash;
use tokio::sync::{mpsc, oneshot};
use tracing::debug;
use utilities::task_scope::{Scope, ScopedJoinHandle};

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
										debug!("Already in pool with tx_hash: {expected_hash:#x}.");
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
			.unwrap() // or cancel
	}
}
