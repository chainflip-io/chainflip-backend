use jsonrpsee::rpc_params;

use super::GetRecentPrioritizationFees;
use crate::traits::Call;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriorizationFees {
	pub slot: u64,
	pub prioritization_fee: u64,
}

impl Call for GetRecentPrioritizationFees {
	type Response = Vec<PriorizationFees>;
	const CALL_METHOD_NAME: &'static str = "getRecentPrioritizationFees";
	fn call_params(&self) -> jsonrpsee::core::params::ArrayParams {
		rpc_params![]
	}
}
