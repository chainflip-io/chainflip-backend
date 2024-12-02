use chainflip_api::{
	primitives::ForeignChain, AddressString, Asset, BrokerApi, ChainflipApi, WithdrawFeesDetail,
};
use jsonrpsee_flatten::types::ArrayParam;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sp_core::{H256, U256};

use super::{ApiWrapper, MockApi};

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

impl api_json_schema::Responder<Endpoint> for MockApi {
	async fn respond(
		&self,
		_request: <Endpoint as api_json_schema::Endpoint>::Request,
	) -> api_json_schema::EndpointResult<Endpoint> {
		Ok(WithdrawFeesDetail {
			tx_hash: H256::random(),
			egress_id: (ForeignChain::Ethereum, 1234),
			egress_amount: U256::from_dec_str("177561759964").unwrap(),
			egress_fee: U256::from_dec_str("7300777").unwrap(),
			destination_address: "0xaa3642e41ca867c0059be41a4df81548fa4424ac".parse().unwrap(),
		})
	}
}
