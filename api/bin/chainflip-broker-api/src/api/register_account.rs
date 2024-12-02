use super::{ApiWrapper, Empty, MockApi};
use chainflip_api::{AccountRole, ChainflipApi, OperatorApi};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sp_core::H256;

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

impl api_json_schema::Endpoint for Endpoint {
	type Request = Empty;
	type Response = RegistrationSuccess;
	type Error = anyhow::Error;
}

impl<T: ChainflipApi> api_json_schema::Responder<Endpoint> for ApiWrapper<T> {
	async fn respond(
		&self,
		_: api_json_schema::EndpointRequest<Endpoint>,
	) -> api_json_schema::EndpointResult<Endpoint> {
		Ok(self.operator_api().register_account_role(AccountRole::Broker).await?.into())
	}
}

impl api_json_schema::Responder<Endpoint> for MockApi {
	async fn respond(
		&self,
		_: api_json_schema::EndpointRequest<Endpoint>,
	) -> api_json_schema::EndpointResult<Endpoint> {
		Ok(RegistrationSuccess { transaction_hash: H256::random() })
	}
}
