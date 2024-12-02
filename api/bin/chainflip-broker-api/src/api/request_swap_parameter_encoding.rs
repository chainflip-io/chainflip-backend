use crate::api::request_swap_deposit_address;
use anyhow::anyhow;
use cf_utilities::rpc::NumberOrHex;
use chainflip_api::{
	primitives::{state_chain_runtime::runtime_apis::VaultSwapDetails, VaultSwapExtraParameters},
	AddressString, BaseRpcApi, ChainflipApi, CustomApiClient,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{ApiWrapper, MockApi};

pub struct Endpoint;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct Request<A> {
	#[serde(flatten)]
	pub inner: request_swap_deposit_address::Request<A>,
	#[serde(flatten)]
	pub extra: VaultSwapExtraParameters<A, cf_utilities::rpc::NumberOrHex>,
}

impl api_json_schema::Endpoint for Endpoint {
	type Request = Request<AddressString>;
	type Response = VaultSwapDetails<AddressString>;
	type Error = anyhow::Error;
}

impl<T: ChainflipApi> api_json_schema::Responder<Endpoint> for ApiWrapper<T> {
	async fn respond(
		&self,
		Request {
			extra,
			inner:
				request_swap_deposit_address::Request {
					source_asset,
					destination_asset,
					destination_address,
					broker_commission,
					channel_metadata,
					boost_fee,
					affiliate_fees,
					refund_parameters: _,
					dca_parameters,
				},
		}: api_json_schema::EndpointRequest<Endpoint>,
	) -> api_json_schema::EndpointResult<Endpoint> {
		Ok(self
			.base_rpc_api()
			.raw_rpc_client()
			.cf_get_vault_swap_details(
				self.account_id(),
				source_asset,
				destination_asset,
				destination_address,
				broker_commission,
				extra,
				channel_metadata,
				boost_fee,
				affiliate_fees,
				dca_parameters,
				None,
			)
			.await?)
	}
}
