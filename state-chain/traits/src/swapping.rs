use cf_chains::{CcmSwapMetadataGeneric, ChannelRefundParameters, ForeignChainAddress, SwapOrigin};
use cf_primitives::{Asset, AssetAmount, Beneficiaries, DcaParameters, SwapRequestId};
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum SwapType {
	Swap,
	CcmPrincipal,
	CcmGas,
	NetworkFee,
	IngressEgressFee,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum SwapRequestTypeGeneric<Address> {
	NetworkFee,
	IngressEgressFee,
	Regular { output_address: Address },
	Ccm { output_address: Address, ccm_swap_metadata: CcmSwapMetadataGeneric<Address> },
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
		refund_params: Option<ChannelRefundParameters>,
		dca_params: Option<DcaParameters>,
		origin: SwapOrigin,
	) -> SwapRequestId;
}
