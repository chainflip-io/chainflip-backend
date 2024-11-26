use crate::{
	api,
	api::{async_trait, request_swap_deposit_address},
};
use chainflip_api::{
	primitives::{state_chain_runtime::runtime_apis::VaultSwapDetails, VaultSwapExtraParameters},
	AddressString, BaseRpcApi, ChainflipApi, CustomApiClient,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub struct Endpoint;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct Request<A> {
	#[serde(flatten)]
	pub inner: request_swap_deposit_address::Request<A>,
	#[serde(flatten)]
	pub extra: VaultSwapExtraParameters<A, cf_utilities::rpc::NumberOrHex>,
}

impl crate::api::Endpoint for Endpoint {
	type Request = Request<AddressString>;
	type Response = VaultSwapDetails<AddressString>;
	type Error = anyhow::Error;
}

#[async_trait]
impl<T: ChainflipApi> api::Responder<Endpoint> for T {
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
		}: api::EndpointRequest<Endpoint>,
	) -> api::EndpointResult<Endpoint> {
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
