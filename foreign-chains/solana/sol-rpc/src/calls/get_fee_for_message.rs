use jsonrpsee::rpc_params;
use serde_json::json;
use sol_prim::Amount;

use super::GetFeeForMessage;
use crate::{traits::Call, types::Commitment};

impl<M> GetFeeForMessage<M> {
	pub fn new(message: M) -> Self {
		Self { message, commitment: Commitment::Confirmed }
	}
}

impl<M> Call for GetFeeForMessage<M>
where
	M: AsRef<[u8]> + Send + Sync,
{
	type Response = Option<Amount>;
	const CALL_METHOD_NAME: &'static str = "getFeeForMessage";
	fn call_params(&self) -> jsonrpsee::core::params::ArrayParams {
		rpc_params![
			self.message.as_ref(),
			json!({
				"commitment": self.commitment,
			})
		]
	}
}
