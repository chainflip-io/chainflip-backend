use super::*;

/// AccountOrAddress is a enum that can represent an internal account or an external address.
/// This is used to represent the destination address for an egress or an internal account
/// to move funds internally.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, PartialOrd, Ord)]
pub enum AccountOrAddress<AccountId, Address> {
	InternalAccount(AccountId),
	ExternalAddress(Address),
}

/// Refund parameter used within the swapping pallet.
#[derive(
	Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Serialize, Deserialize,
)]
pub struct SwapRefundParameters {
	pub refund_block: cf_primitives::BlockNumber,
	pub min_output: cf_primitives::AssetAmount,
}

#[derive(
	Clone,
	Debug,
	PartialEq,
	Eq,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
	Serialize,
	Deserialize,
	PartialOrd,
	Ord,
)]
/// Generic types for Unchecked version of Refund Parameters. Avoid using this type directly, and
/// always use the appropriate type-aliased version.
pub struct ChannelRefundParameters<A, RefundCcm = Option<CcmChannelMetadataUnchecked>> {
	pub retry_duration: cf_primitives::BlockNumber,
	pub refund_address: A,
	pub min_price: Price,
	pub refund_ccm_metadata: RefundCcm,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, PartialOrd, Ord)]
pub struct RefundParametersCheckedGeneric<Address, AccountId> {
	pub retry_duration: cf_primitives::BlockNumber,
	pub refund_destination: AccountOrAddress<AccountId, Address>,
	pub min_price: Price,
	pub refund_ccm_metadata: Option<CcmDepositMetadataChecked<Address>>,
}

pub type RefundParametersChecked<AccountId> =
	RefundParametersCheckedGeneric<ForeignChainAddress, AccountId>;

impl<AccountId> RefundParametersChecked<AccountId> {
	pub fn min_output_amount(&self, input_amount: AssetAmount) -> AssetAmount {
		use sp_runtime::traits::UniqueSaturatedInto;
		cf_amm_math::output_amount_ceil(input_amount.into(), self.min_price).unique_saturated_into()
	}

	pub fn try_from_refund_parameters<Converter: AddressConverter>(
		refund_param: ChannelRefundParametersEncoded,
		source_address: Option<ForeignChainAddress>,
		refund_asset: Asset,
	) -> Result<Self, DispatchError> {
		Self::try_from_refund_parameters_internal(
			refund_param.try_map_address(|addr| {
				Converter::try_from_encoded_address(addr).map_err(|_| "Invalid refund address")
			})?,
			source_address,
			refund_asset,
		)
	}

	pub fn try_from_refund_parameters_for_chain<C: Chain>(
		refund_param: ChannelRefundParametersForChain<C>,
		source_address: Option<ForeignChainAddress>,
		refund_asset: Asset,
	) -> Result<Self, DispatchError> {
		Self::try_from_refund_parameters_internal(
			refund_param.map_address(|addr| addr.into_foreign_chain_address()),
			source_address,
			refund_asset,
		)
	}

	fn try_from_refund_parameters_internal(
		refund_param: ChannelRefundParameters<ForeignChainAddress>,
		source_address: Option<ForeignChainAddress>,
		refund_asset: Asset,
	) -> Result<Self, DispatchError> {
		if refund_param.refund_ccm_metadata.is_some() &&
			!refund_param.refund_address.chain().ccm_support()
		{
			return Err("Invalid refund parameter: Ccm not supported for the refund chain.".into())
		}

		Ok(RefundParametersChecked::<AccountId> {
			retry_duration: refund_param.retry_duration,
			refund_destination: AccountOrAddress::ExternalAddress(
				refund_param.refund_address.clone(),
			),
			min_price: refund_param.min_price,
			refund_ccm_metadata: refund_param
				.refund_ccm_metadata
				.map(|ccm| {
					CcmDepositMetadataUnchecked {
						channel_metadata: ccm,
						source_chain: refund_param.refund_address.chain(),
						source_address,
					}
					.to_checked(refund_asset, refund_param.refund_address)
				})
				.transpose()?,
		})
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl<A: BenchmarkValue> BenchmarkValue for ChannelRefundParameters<A> {
	fn benchmark_value() -> Self {
		Self {
			retry_duration: BenchmarkValue::benchmark_value(),
			refund_address: BenchmarkValue::benchmark_value(),
			min_price: BenchmarkValue::benchmark_value(),
			refund_ccm_metadata: Some(BenchmarkValue::benchmark_value()),
		}
	}
}
pub type ChannelRefundParametersLegacy<RefundAddress> = ChannelRefundParameters<RefundAddress, ()>;
pub type ChannelRefundParametersEncoded = ChannelRefundParameters<EncodedAddress>;
pub type ChannelRefundParametersForChain<C> = ChannelRefundParameters<<C as Chain>::ChainAccount>;

impl<A: Clone, RefundCcm: Clone> ChannelRefundParameters<A, RefundCcm> {
	pub fn map_address<B, F: FnOnce(A) -> B>(&self, f: F) -> ChannelRefundParameters<B, RefundCcm> {
		ChannelRefundParameters {
			retry_duration: self.retry_duration,
			refund_address: f(self.refund_address.clone()),
			min_price: self.min_price,
			refund_ccm_metadata: self.refund_ccm_metadata.clone(),
		}
	}
	pub fn try_map_address<B, E, F: FnOnce(A) -> Result<B, E>>(
		&self,
		f: F,
	) -> Result<ChannelRefundParameters<B, RefundCcm>, E> {
		Ok(ChannelRefundParameters {
			retry_duration: self.retry_duration,
			refund_address: f(self.refund_address.clone())?,
			min_price: self.min_price,
			refund_ccm_metadata: self.refund_ccm_metadata.clone(),
		})
	}
}

impl<RefundAddress> ChannelRefundParameters<RefundAddress> {
	pub fn validate(
		&self,
		refund_asset: Asset,
		refund_address_decoded: ForeignChainAddress,
	) -> Result<(), DispatchError> {
		self.refund_ccm_metadata
			.as_ref()
			.map(|refund_ccm| {
				CcmValidityChecker::check_and_decode(
					refund_ccm,
					refund_asset,
					refund_address_decoded,
				)
			})
			.transpose()?;

		Ok(())
	}
}
