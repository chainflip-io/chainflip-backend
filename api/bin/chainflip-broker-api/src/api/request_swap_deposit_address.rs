use crate::api;
use chainflip_api::{
	self,
	primitives::{
		Affiliates, Asset, BasisPoints, CcmChannelMetadata, DcaParameters, RefundParameters,
	},
	AccountId32, AddressString, Beneficiary, BrokerApi, ChainflipApi, SwapDepositAddress,
};
use jsonrpsee::core::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, JsonSchema, Serialize, Deserialize)]
pub struct Request<A> {
	pub source_asset: Asset,
	pub destination_asset: Asset,
	pub destination_address: AddressString,
	pub broker_commission: BasisPoints,
	pub channel_metadata: Option<CcmChannelMetadata>,
	pub boost_fee: Option<BasisPoints>,
	#[schemars(with = "Vec<Beneficiary<cf_utilities::json_schema::AccountId32>>")]
	#[schemars(range(min = 0, max = 2))]
	pub affiliate_fees: Option<Affiliates<AccountId32>>,
	pub refund_parameters: Option<RefundParameters<A>>,
	pub dca_parameters: Option<DcaParameters>,
}

pub struct Endpoint;

impl api::Endpoint for Endpoint {
	type Request = Request<AddressString>;
	type Response = SwapDepositAddress;
	type Error = anyhow::Error;
}

#[async_trait]
impl<T: ChainflipApi> api::Responder<Endpoint> for T {
	async fn respond(
		&self,
		Request {
			source_asset,
			destination_asset,
			destination_address,
			broker_commission,
			channel_metadata,
			boost_fee,
			affiliate_fees,
			refund_parameters,
			dca_parameters,
		}: api::EndpointRequest<Endpoint>,
	) -> api::EndpointResult<Endpoint> {
		Ok(self
			.broker_api()
			.request_swap_deposit_address(
				source_asset,
				destination_asset,
				destination_address,
				broker_commission,
				channel_metadata,
				boost_fee,
				affiliate_fees,
				refund_parameters,
				dca_parameters,
			)
			.await?)
	}
}
