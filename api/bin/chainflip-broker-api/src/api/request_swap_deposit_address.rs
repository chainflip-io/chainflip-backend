use crate::api;
use chainflip_api::{
	self,
	primitives::{
		Affiliates, Asset, BasisPoints, CcmChannelMetadata, DcaParameters, RefundParameters,
	},
	AccountId32, AddressString, Beneficiary, BrokerApi, ChainflipApi, SwapDepositAddress,
};
use jsonrpsee::core::async_trait;
use jsonrpsee_flatten::types::ArrayParam;
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

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

impl<A: Clone + Serialize + DeserializeOwned> ArrayParam for Request<A> {
	type ArrayTuple = (
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

	fn into_array_tuple(self) -> Self::ArrayTuple {
		(
			self.source_asset,
			self.destination_asset,
			self.destination_address,
			self.broker_commission,
			self.channel_metadata,
			self.boost_fee,
			self.affiliate_fees,
			self.refund_parameters,
			self.dca_parameters,
		)
	}

	fn from_array_tuple(
		(
			source_asset,
			destination_asset,
			destination_address,
			broker_commission,
			channel_metadata,
			boost_fee,
			affiliate_fees,
			refund_parameters,
			dca_parameters,
		): Self::ArrayTuple,
	) -> Self {
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
		}
	}
}
