use jsonrpsee::rpc_params;
use serde_json::json;

use sol_prim::{Address, Signature};

use super::GetSignaturesForAddress;
use crate::{
	traits::Call,
	types::{Commitment, JsValue},
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureForAddress {
	pub block_time: u64,
	pub slot: u64,
	pub signature: Signature,
	pub confirmation_status: Commitment,
	pub err: Option<JsValue>,
	pub memo: Option<JsValue>,
}

impl GetSignaturesForAddress {
	pub fn for_address(address: Address) -> Self {
		Self {
			address,

			commitment: Default::default(),
			before: None,
			until: None,
			limit: None,
			min_context_slot: None,
		}
	}
}

impl Call for GetSignaturesForAddress {
	type Response = Vec<SignatureForAddress>;
	const CALL_METHOD_NAME: &'static str = "getSignaturesForAddress";
	fn call_params(&self) -> jsonrpsee::core::params::ArrayParams {
		let address = self.address.to_string();
		rpc_params![
			address.as_str(),
			json!({
				"commitment": self.commitment,
				"before": self.before,
				"until": self.until,
				"limit": self.limit,
				"min_context_slot": self.min_context_slot,
			})
		]
	}

	fn process_response(&self, input: JsValue) -> Result<Self::Response, serde_json::Error> {
		let mut entries: Vec<SignatureForAddress> = serde_json::from_value(input)?;
		entries.sort_by_key(|e| std::cmp::Reverse(e.slot));

		Ok(entries)
	}
}
