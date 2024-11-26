use crate::api;
use chainflip_api::{AccountRole, ChainflipApi, OperatorApi};
use jsonrpsee::core::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sp_core::H256;

type Request = ();

#[derive(Debug, Clone, JsonSchema, Serialize, Deserialize)]
#[schemars(description = "Account registration success.")]
/// Account registration success doc comment.
pub struct RegistrationSuccess {
	/// A 32-byte hash encoded as a `0x`-prefixed hex string.
	#[schemars(schema_with = "cf_utilities::json_schema::hex_array::<32>")]
	pub transaction_hash: H256,
}

impl From<H256> for RegistrationSuccess {
	fn from(transaction_hash: H256) -> Self {
		Self { transaction_hash }
	}
}

pub struct Endpoint;

impl api::Endpoint for Endpoint {
	type Request = Request;
	type Response = RegistrationSuccess;
	type Error = anyhow::Error;
}

#[async_trait]
impl<T: ChainflipApi> api::Responder<Endpoint> for T {
	async fn respond(&self, _: api::EndpointRequest<Endpoint>) -> api::EndpointResult<Endpoint> {
		Ok(self.operator_api().register_account_role(AccountRole::Broker).await?.into())
	}
}
