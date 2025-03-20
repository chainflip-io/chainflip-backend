use crate::{
	address::ForeignChainAddress, ccm_checker::DecodedCcmAdditionalData, none::NoneChainCrypto,
	sol::SolanaAltLookup, CcmAuxDataLookupKeyConversion, Chain, DepositDetailsToTransactionInId,
	FeeRefundCalculator,
};
use codec::{Decode, Encode, FullCodec, MaxEncodedLen};
use frame_support::Parameter;
use scale_info::TypeInfo;
use sp_runtime::{traits::Member, RuntimeDebug};

use crate::benchmarking_value::BenchmarkValue;
use cf_primitives::{
	chains::{assets, AnyChain},
	AssetAmount, ChannelId, SwapRequestId,
};

impl Chain for AnyChain {
	const NAME: &'static str = "AnyChain";
	const GAS_ASSET: Self::ChainAsset = assets::any::Asset::Usdc;
	const WITNESS_PERIOD: u64 = 1;

	type ChainCrypto = NoneChainCrypto;
	type ChainBlockNumber = u64;
	type ChainAmount = AssetAmount;
	type TransactionFee = Self::ChainAmount;
	type TrackedData = ();
	type ChainAsset = assets::any::Asset;
	type ChainAssetMap<
		T: Member + Parameter + MaxEncodedLen + Copy + BenchmarkValue + FullCodec + Unpin,
	> = assets::any::AssetMap<T>;
	type ChainAccount = ForeignChainAddress;
	type DepositFetchId = ChannelId;
	type DepositChannelState = ();
	type DepositDetails = ();
	type Transaction = ();
	type TransactionMetadata = ();
	type TransactionRef = ();
	type ReplayProtectionParams = ();
	type ReplayProtection = ();
	type CcmAuxDataLookupKey = AnyChainCcmAuxDataLookupKey;
}

impl FeeRefundCalculator<AnyChain> for () {
	fn return_fee_refund(
		&self,
		_fee_paid: <AnyChain as Chain>::TransactionFee,
	) -> <AnyChain as Chain>::ChainAmount {
		unimplemented!()
	}
}

impl DepositDetailsToTransactionInId<NoneChainCrypto> for () {}

#[derive(Encode, Decode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
pub enum AnyChainCcmAuxDataLookupKey {
	Solana(SolanaAltLookup),
}

impl From<AnyChainCcmAuxDataLookupKey> for () {
	fn from(_value: AnyChainCcmAuxDataLookupKey) {}
}

impl From<AnyChainCcmAuxDataLookupKey> for SolanaAltLookup {
	fn from(value: AnyChainCcmAuxDataLookupKey) -> SolanaAltLookup {
		match value {
			AnyChainCcmAuxDataLookupKey::Solana(lookup) => lookup,
		}
	}
}

impl AnyChainCcmAuxDataLookupKey {
	pub fn into_lookup_key(
		decoded_ccm_data: DecodedCcmAdditionalData,
		swap_request_id: SwapRequestId,
		created_at: u32,
	) -> Option<Self> {
		if let DecodedCcmAdditionalData::Solana(sol_ccm_data) = decoded_ccm_data {
			(!sol_ccm_data.address_lookup_tables().is_empty()).then_some(
				AnyChainCcmAuxDataLookupKey::Solana(SolanaAltLookup::from_alt_lookup_key(
					swap_request_id,
					created_at,
				)),
			)
		} else {
			None
		}
	}
}

impl CcmAuxDataLookupKeyConversion for AnyChainCcmAuxDataLookupKey {
	fn from_alt_lookup_key(swap_request_id: SwapRequestId, created_at: u32) -> Self {
		AnyChainCcmAuxDataLookupKey::Solana(SolanaAltLookup::from_alt_lookup_key(
			swap_request_id,
			created_at,
		))
	}
}
