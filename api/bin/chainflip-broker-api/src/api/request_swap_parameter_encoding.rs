use crate::{
	api,
	api::{async_trait, request_swap_deposit_address},
};
use chainflip_api::{
	primitives::{state_chain_runtime::runtime_apis::VaultSwapDetails, AssetAmount},
	AddressString, BaseRpcApi, ChainflipApi, CustomApiClient,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub struct Endpoint;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct Request<A> {
	pub input_amount: AssetAmount,
	#[serde(flatten)]
	pub inner: request_swap_deposit_address::Request<A>,
}

impl crate::api::Endpoint for Endpoint {
	type Request = Request<()>;
	type Response = VaultSwapDetails<AddressString>;
	type Error = anyhow::Error;
}

#[async_trait]
impl<T: ChainflipApi> api::Responder<Endpoint> for T {
	async fn respond(
		&self,
		Request {
			input_amount,
			inner:
				request_swap_deposit_address::Request {
					source_asset,
					destination_asset,
					destination_address,
					broker_commission,
					channel_metadata: _,
					boost_fee,
					affiliate_fees,
					refund_parameters,
					dca_parameters,
				},
		}: api::EndpointRequest<Endpoint>,
	) -> api::EndpointResult<Endpoint> {
		// TODO: Use refund params including address in the runtime rpc. Make refund address
		// mandatory.
		let min_output_amount = refund_parameters
			.as_ref()
			.map(|refund_parameters| refund_parameters.min_output_amount(input_amount))
			.unwrap_or_default();
		let retry_duration = refund_parameters
			.as_ref()
			.map(|refund_parameters| refund_parameters.retry_duration)
			.unwrap_or(2);
		Ok(self
			.base_rpc_api()
			.raw_rpc_client()
			.cf_get_vault_swap_details(
				self.account_id(),
				source_asset,
				destination_asset,
				destination_address,
				broker_commission,
				min_output_amount,
				retry_duration,
				boost_fee,
				affiliate_fees,
				dca_parameters,
				None,
			)
			.await?)
	}
}
