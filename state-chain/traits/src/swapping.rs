use cf_chains::{CcmDepositMetadata, ChannelRefundParameters, ForeignChainAddress, SwapOrigin};
use cf_primitives::{Asset, AssetAmount, Beneficiaries, SwapRequestId};
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::DispatchError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum SwapType {
	Swap,
	CcmPrincipal,
	CcmGas,
	NetworkFee,
	IngressEgressFee,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum SwapRequestType {
	NetworkFee,
	IngressEgressFee,
	Regular { output_address: ForeignChainAddress },
	Ccm { output_address: ForeignChainAddress, ccm_deposit_metadata: CcmDepositMetadata },
}

pub trait SwapRequestHandler {
	type AccountId;

	fn init_swap_request(
		input_asset: Asset,
		input_amount: AssetAmount,
		output_asset: Asset,
		request_type: SwapRequestType,
		broker_fees: Beneficiaries<Self::AccountId>,
		refund_params: Option<ChannelRefundParameters>,
		origin: SwapOrigin,
	) -> Result<SwapRequestId, DispatchError>;
}
