use jsonrpsee::rpc_params;
use sol_prim::Digest;

use super::GetGenesisHash;
use crate::traits::Call;

impl Call for GetGenesisHash {
	type Response = Digest;
	const CALL_METHOD_NAME: &'static str = "getGenesisHash";
	fn call_params(&self) -> jsonrpsee::core::params::ArrayParams {
		rpc_params![]
	}
}
