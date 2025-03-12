use crate::{
	address::ForeignChainAddress, none::NoneChainCrypto, sol::SolanaAltLookup,
	CcmAuxDataLookupKeyConversion, Chain, DepositDetailsToTransactionInId, FeeRefundCalculator,
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
	Others,
}

impl TryInto<()> for AnyChainCcmAuxDataLookupKey {
	type Error = ();
	fn try_into(self) -> Result<(), Self::Error> {
		Ok(())
	}
}

impl TryInto<SolanaAltLookup> for AnyChainCcmAuxDataLookupKey {
	type Error = ();
	fn try_into(self) -> Result<SolanaAltLookup, Self::Error> {
		if let AnyChainCcmAuxDataLookupKey::Solana(lookup) = self {
			Ok(lookup)
		} else {
			Err(())
		}
	}
}

impl CcmAuxDataLookupKeyConversion for AnyChainCcmAuxDataLookupKey {
	fn created_at(&self) -> Option<u32> {
		if let AnyChainCcmAuxDataLookupKey::Solana(lookup) = self {
			lookup.created_at()
		} else {
			None
		}
	}
	fn from_alt_lookup_key(swap_request_id: SwapRequestId, created_at: u32) -> Self {
		AnyChainCcmAuxDataLookupKey::Solana(SolanaAltLookup::from_alt_lookup_key(
			swap_request_id,
			created_at,
		))
	}
}
