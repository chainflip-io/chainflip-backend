use api_json_schema::{self};
use chainflip_api::{
	self,
	primitives::{
		Affiliates, Asset, BasisPoints, CcmChannelMetadata, DcaParameters, RefundParameters,
	},
	AccountId32, AddressString, Beneficiary, BrokerApi, ChainflipApi, SwapDepositAddress,
};
use jsonrpsee_flatten::types::ArrayParam;
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use super::{ApiWrapper, MockApi};

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

impl api_json_schema::Endpoint for Endpoint {
	type Request = Request<AddressString>;
	type Response = SwapDepositAddress;
	type Error = anyhow::Error;
}

impl<T: ChainflipApi> api_json_schema::Responder<Endpoint> for ApiWrapper<T> {
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
		}: api_json_schema::EndpointRequest<Endpoint>,
	) -> api_json_schema::EndpointResult<Endpoint> {
		self.broker_api()
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
			.await
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

impl api_json_schema::Responder<Endpoint> for MockApi {
	async fn respond(
		&self,
		_request: <Endpoint as api_json_schema::Endpoint>::Request,
	) -> api_json_schema::EndpointResult<Endpoint> {
		Ok(SwapDepositAddress {
			address: "bc1pw75mqye4q9t0m649vtk0a9clsrf2fagq3mr6agekfzsx0lulfyxsvxqadt"
				.parse()
				.unwrap(),
			issued_block: 5_529_592,
			channel_id: 20902,
			source_chain_expiry_block: 873041u64.into(),
			channel_opening_fee: 0.into(),
			refund_parameters: Some(RefundParameters {
				retry_duration: 10,
				refund_address: "bc1qx4cgzlxlk0vukvhfhtp5j6qr75af3ma9q4g53j".parse().unwrap(),
				min_price: sp_core::U256::from_dec_str(
					"320545968868688355728415070396339617556420",
				)
				.unwrap(),
			}),
		})
	}
}
