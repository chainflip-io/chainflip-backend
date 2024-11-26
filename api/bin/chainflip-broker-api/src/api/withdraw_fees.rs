use crate::api;
use chainflip_api::{AddressString, Asset, BrokerApi, ChainflipApi, WithdrawFeesDetail};
use jsonrpsee::core::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Request {
	asset: Asset,
	destination_address: AddressString,
}

pub struct Endpoint;

impl api::Endpoint for Endpoint {
	type Request = Request;
	type Response = WithdrawFeesDetail;
	type Error = anyhow::Error;
}

#[async_trait]
impl<T: ChainflipApi> api::Responder<Endpoint> for T {
	async fn respond(
		&self,
		Request { asset, destination_address }: Request,
	) -> api::EndpointResult<Endpoint> {
		Ok(self.broker_api().withdraw_fees(asset, destination_address).await?)
	}
}
