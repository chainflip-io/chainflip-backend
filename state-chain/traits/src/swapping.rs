use cf_chains::{
	address::{AddressConverter, EncodedAddress},
	AccountOrAddress, CcmDepositMetadataGeneric, Chain, ForeignChainAddress,
	RefundParametersExtended, SwapOrigin,
};
use cf_primitives::{
	Asset, AssetAmount, Beneficiaries, BlockNumber, DcaParameters, Price, SwapRequestId,
};
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
	type AccountId: Clone;

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

	fn init_network_fee_swap_request(
		input_asset: Asset,
		input_amount: AssetAmount,
	) -> SwapRequestId {
		Self::init_swap_request(
			input_asset,
			input_amount,
			Asset::Flip,
			SwapRequestType::NetworkFee,
			Default::default(), /* broker fees */
			None,               /* refund params */
			None,               /* dca params */
			SwapOrigin::Internal,
		)
	}

	fn init_ingress_egress_fee_swap_request<C: Chain>(
		input_asset: C::ChainAsset,
		input_amount: C::ChainAmount,
	) -> SwapRequestId {
		Self::init_swap_request(
			input_asset.into(),
			input_amount.into(),
			C::GAS_ASSET.into(),
			SwapRequestType::IngressEgressFee,
			Default::default(), /* broker fees */
			None,               /* refund params */
			None,               /* dca params */
			SwapOrigin::Internal,
		)
	}

	fn init_internal_swap_request(
		input_asset: Asset,
		input_amount: AssetAmount,
		output_asset: Asset,
		retry_duration: BlockNumber,
		min_price: Price,
		dca_params: Option<DcaParameters>,
		account_id: Self::AccountId,
	) -> SwapRequestId {
		Self::init_swap_request(
			input_asset,
			input_amount,
			output_asset,
			SwapRequestType::Regular {
				output_action: SwapOutputAction::CreditOnChain { account_id: account_id.clone() },
			},
			Default::default(), /* no broker fees */
			Some(RefundParametersExtended {
				retry_duration,
				refund_destination: AccountOrAddress::InternalAccount(account_id.clone()),
				min_price,
			}),
			dca_params,
			SwapOrigin::OnChainAccount(account_id),
		)
	}
}
