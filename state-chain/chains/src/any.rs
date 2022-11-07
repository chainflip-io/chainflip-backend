use crate::Chain;
use cf_primitives::chains::{assets, AnyChain};

impl Chain for AnyChain {
	type ChainBlockNumber = u64;
	type ChainAmount = u128;
	type TransactionFee = Self::ChainAmount;
	type TrackedData = ();
	type ChainAsset = assets::any::Asset;
	type ChainAccount = ();
}
