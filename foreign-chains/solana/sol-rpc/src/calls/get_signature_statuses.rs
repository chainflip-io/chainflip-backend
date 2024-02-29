use jsonrpsee::rpc_params;
use serde_json::json;

use sol_prim::{Address, Signature};

use super::GetSignatureStatuses;
use crate::{
	traits::Call,
	types::{Commitment, JsValue},
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureStatus {
	pub slot: u64,
	pub confirmations: Option<u64>,
	pub confirmation_status: Commitment,
	pub err: Option<JsValue>,
}

impl Call for GetSignatureStatuses {
	type Response = Vec<Option<SignatureStatus>>;
	const CALL_METHOD_NAME: &'static str = "getSignatureStatuses";
	fn call_params(&self) -> jsonrpsee::core::params::ArrayParams {
		rpc_params![
			signatures,
			json!({
				"search_transaction_history": self.search_transaction_history,
			})
		]
	}
}
