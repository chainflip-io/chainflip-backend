use crate::Chain;
use cf_primitives::{
	chains::{assets, AnyChain},
	AssetAmount, ForeignChainAddress,
};

impl Chain for AnyChain {
	type ChainBlockNumber = u64;
	type ChainAmount = AssetAmount;
	type TransactionFee = Self::ChainAmount;
	type TrackedData = ();
	type ChainAsset = assets::any::Asset;
	type ChainAccount = ForeignChainAddress;
	type EpochStartData = ();
}
