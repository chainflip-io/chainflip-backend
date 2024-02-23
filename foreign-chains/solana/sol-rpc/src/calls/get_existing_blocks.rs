use jsonrpsee::rpc_params;
use sol_prim::SlotNumber;

use crate::traits::Call;

use super::GetExistingBlocks;

impl Call for GetExistingBlocks {
	type Response = Vec<SlotNumber>;
	const CALL_METHOD_NAME: &'static str = "getBlocks";
	fn call_params(&self) -> jsonrpsee::core::params::ArrayParams {
		rpc_params![self.lo, self.hi]
	}
}

impl GetExistingBlocks {
	pub fn range(lo: SlotNumber, hi: SlotNumber) -> Self {
		Self { lo, hi }
	}
}
