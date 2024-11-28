use crate::api::request_swap_deposit_address;
use chainflip_api::{
	primitives::{
		state_chain_runtime::runtime_apis::VaultSwapDetails, AssetAmount, CcmChannelMetadata,
		DcaParameters, RefundParameters,
	},
	AccountId32, AddressString, Affiliates, Asset, BaseRpcApi, BasisPoints, ChainflipApi,
	CustomApiClient,
};
use jsonrpsee_flatten::types::ArrayParam;
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use super::ApiWrapper;

pub struct Endpoint;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct Request<A> {
	pub input_amount: AssetAmount,
	#[serde(flatten)]
	pub inner: request_swap_deposit_address::Request<A>,
}

impl api_json_schema::Endpoint for Endpoint {
	type Request = Request<()>;
	type Response = VaultSwapDetails<AddressString>;
	type Error = anyhow::Error;
}

impl<T: ChainflipApi> api_json_schema::Responder<Endpoint> for ApiWrapper<T> {
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
		}: api_json_schema::EndpointRequest<Endpoint>,
	) -> api_json_schema::EndpointResult<Endpoint> {
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

impl<A: Clone + Serialize + DeserializeOwned> ArrayParam for Request<A> {
	type ArrayTuple = (
		AssetAmount,
		Asset,
		Asset,
		AddressString,
		BasisPoints,
		Option<CcmChannelMetadata>,
		Option<BasisPoints>,
		Option<Affiliates<AccountId32>>,
		Option<RefundParameters<A>>,
		Option<DcaParameters>,
	);

	fn into_array_tuple(self) -> <Self as ArrayParam>::ArrayTuple {
		(
			self.input_amount,
			self.inner.source_asset,
			self.inner.destination_asset,
			self.inner.destination_address,
			self.inner.broker_commission,
			self.inner.channel_metadata,
			self.inner.boost_fee,
			self.inner.affiliate_fees,
			self.inner.refund_parameters,
			self.inner.dca_parameters,
		)
	}

	fn from_array_tuple(
		(
			input_amount,
			source_asset,
			destination_asset,
			destination_address,
			broker_commission,
			channel_metadata,
			boost_fee,
			affiliate_fees,
			refund_parameters,
			dca_parameters,
		): <Self as ArrayParam>::ArrayTuple,
	) -> Self {
		Request {
			input_amount,
			inner: request_swap_deposit_address::Request {
				source_asset,
				destination_asset,
				destination_address,
				broker_commission,
				channel_metadata,
				boost_fee,
				affiliate_fees,
				refund_parameters,
				dca_parameters,
			},
		}
	}
}
