use jsonrpsee::rpc_params;
use serde_json::json;
use sol_prim::Digest;

use super::GetLatestBlockhash;
use crate::{traits::Call, types::WithContext};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LatestBlockhash {
	pub blockhash: Digest,
	pub last_valid_block_height: u64,
}

impl Call for GetLatestBlockhash {
	type Response = WithContext<LatestBlockhash>;
	const CALL_METHOD_NAME: &'static str = "getLatestBlockhash";

	fn call_params(&self) -> jsonrpsee::core::params::ArrayParams {
		rpc_params![json!({
			"commitment": self.commitment,
		})]
	}
}
