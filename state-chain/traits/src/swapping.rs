use cf_chains::{
	CcmDepositMetadataGeneric, ChannelRefundParametersDecoded, ForeignChainAddress, SwapOrigin,
};
use cf_primitives::{Asset, AssetAmount, Beneficiaries, DcaParameters, SwapRequestId};
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum SwapType {
	Swap,
	NetworkFee,
	IngressEgressFee,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum SwapRequestTypeGeneric<Address> {
	NetworkFee,
	IngressEgressFee,
	Regular {
		output_address: Address,
		ccm_deposit_metadata: Option<CcmDepositMetadataGeneric<Address>>,
	},
}

pub type SwapRequestType = SwapRequestTypeGeneric<ForeignChainAddress>;
pub type SwapRequestTypeEncoded = SwapRequestTypeGeneric<cf_chains::address::EncodedAddress>;

pub trait SwapRequestHandler {
	type AccountId;

	fn init_swap_request(
		input_asset: Asset,
		input_amount: AssetAmount,
		output_asset: Asset,
		request_type: SwapRequestType,
		broker_fees: Beneficiaries<Self::AccountId>,
		refund_params: Option<ChannelRefundParametersDecoded>,
		dca_params: Option<DcaParameters>,
		origin: SwapOrigin,
	) -> SwapRequestId;
}
