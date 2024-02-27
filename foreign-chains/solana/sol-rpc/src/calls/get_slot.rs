use jsonrpsee::rpc_params;
use serde_json::json;
use sol_prim::SlotNumber;

use crate::traits::Call;

use super::GetSlot;

impl Call for GetSlot {
	type Response = SlotNumber;
	const CALL_METHOD_NAME: &'static str = "getSlot";
	fn call_params(&self) -> jsonrpsee::core::params::ArrayParams {
		rpc_params![json!({
			"commitment": self.commitment
		})]
	}
}
