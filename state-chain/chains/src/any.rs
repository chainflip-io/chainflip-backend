use crate::{
	address::{ForeignChainAddress, IntoForeignChainAddress},
	none::NoneChainCrypto,
	Chain, FeeRefundCalculator,
};
use codec::{FullCodec, MaxEncodedLen};
use frame_support::Parameter;
use sp_runtime::traits::{MaybeSerializeDeserialize, Member};

use crate::benchmarking_value::BenchmarkValue;
use cf_primitives::{
	chains::{assets, AnyChain},
	AssetAmount, ChannelId,
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
		T: Member
			+ Parameter
			+ MaxEncodedLen
			+ Copy
			+ MaybeSerializeDeserialize
			+ BenchmarkValue
			+ FullCodec
			+ Unpin
			+ Default,
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
}

impl FeeRefundCalculator<AnyChain> for () {
	fn return_fee_refund(
		&self,
		_fee_paid: <AnyChain as Chain>::TransactionFee,
	) -> <AnyChain as Chain>::ChainAmount {
		unimplemented!()
	}
}

impl IntoForeignChainAddress<AnyChain> for ForeignChainAddress {
	fn into_foreign_chain_address(address: ForeignChainAddress) -> ForeignChainAddress {
		address
	}
}
