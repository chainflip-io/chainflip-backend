use chainflip_api::{AddressString, Asset, BrokerApi, ChainflipApi, WithdrawFeesDetail};
use jsonrpsee_flatten::types::ArrayParam;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::ApiWrapper;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Request {
	asset: Asset,
	destination_address: AddressString,
}

pub struct Endpoint;

impl api_json_schema::Endpoint for Endpoint {
	type Request = Request;
	type Response = WithdrawFeesDetail;
	type Error = anyhow::Error;
}

impl<T: ChainflipApi> api_json_schema::Responder<Endpoint> for ApiWrapper<T> {
	async fn respond(
		&self,
		Request { asset, destination_address }: Request,
	) -> api_json_schema::EndpointResult<Endpoint> {
		self.broker_api().withdraw_fees(asset, destination_address).await
	}
}

impl ArrayParam for Request {
	type ArrayTuple = (Asset, AddressString);

	fn into_array_tuple(self) -> Self::ArrayTuple {
		(self.asset, self.destination_address.clone())
	}

	fn from_array_tuple((asset, destination_address): Self::ArrayTuple) -> Self {
		Self { asset, destination_address }
	}
}
