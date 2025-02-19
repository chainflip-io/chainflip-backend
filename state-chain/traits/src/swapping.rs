use cf_chains::{
	address::{AddressConverter, EncodedAddress},
	CcmDepositMetadataGeneric, ForeignChainAddress, RefundParametersExtended, SwapOrigin,
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

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum SwapOutputActionGeneric<Address, AccountId> {
	Egress {
		ccm_deposit_metadata: Option<CcmDepositMetadataGeneric<Address>>,
		output_address: Address,
	},
	CreditOnChain {
		account_id: AccountId,
	},
}

pub type SwapOutputAction<AccountId> = SwapOutputActionGeneric<ForeignChainAddress, AccountId>;
pub type SwapOutputActionEncoded<AccountId> = SwapOutputActionGeneric<EncodedAddress, AccountId>;

impl<AccountId> SwapRequestType<AccountId> {
	pub fn into_encoded<Converter: AddressConverter>(self) -> SwapRequestTypeEncoded<AccountId> {
		match self {
			SwapRequestType::NetworkFee => SwapRequestTypeEncoded::NetworkFee,
			SwapRequestType::IngressEgressFee => SwapRequestTypeEncoded::IngressEgressFee,
			SwapRequestType::Regular { output_action } => SwapRequestTypeEncoded::Regular {
				output_action: match output_action {
					SwapOutputAction::Egress { ccm_deposit_metadata, output_address } =>
						SwapOutputActionEncoded::Egress {
							output_address: Converter::to_encoded_address(output_address),
							ccm_deposit_metadata: ccm_deposit_metadata
								.map(|metadata| metadata.to_encoded::<Converter>()),
						},
					SwapOutputAction::CreditOnChain { account_id } =>
						SwapOutputActionEncoded::CreditOnChain { account_id },
				},
			},
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum SwapRequestTypeGeneric<Address, AccountId> {
	NetworkFee,
	IngressEgressFee,
	Regular { output_action: SwapOutputActionGeneric<Address, AccountId> },
}

pub type SwapRequestType<AccountId> = SwapRequestTypeGeneric<ForeignChainAddress, AccountId>;
pub type SwapRequestTypeEncoded<AccountId> = SwapRequestTypeGeneric<EncodedAddress, AccountId>;

pub trait SwapRequestHandler {
	type AccountId;

	fn init_swap_request(
		input_asset: Asset,
		input_amount: AssetAmount,
		output_asset: Asset,
		request_type: SwapRequestType<Self::AccountId>,
		broker_fees: Beneficiaries<Self::AccountId>,
		refund_params: Option<RefundParametersExtended<Self::AccountId>>,
		dca_params: Option<DcaParameters>,
		origin: SwapOrigin<Self::AccountId>,
	) -> SwapRequestId;
}
