use crate::{address::ForeignChainAddress, Chain};

use cf_primitives::{
	chains::{assets, AnyChain},
	AssetAmount, ChannelId,
};
use frame_support::traits::ConstBool;

impl Chain for AnyChain {
	const NAME: &'static str = "AnyChain";
	type KeyHandoverIsRequired = ConstBool<false>;
	type OptimisticActivation = ConstBool<true>;
	type ChainBlockNumber = u64;
	type ChainAmount = AssetAmount;
	type TransactionFee = Self::ChainAmount;
	type TrackedData = ();
	type ChainAsset = assets::any::Asset;
	type ChainAccount = ForeignChainAddress;
	type EpochStartData = ();
	type DepositFetchId = ChannelId;
	type DepositChannelState = ();
}
