use crate::Chain;
use cf_primitives::chains::{assets, AnyChain};
use sp_core::Bytes;

impl Chain for AnyChain {
	type ChainBlockNumber = u64;
	type ChainAmount = u128;
	type TransactionFee = Self::ChainAmount;
	type TrackedData = ();
	type ChainAsset = assets::any::Asset;
	type ChainAccount = Bytes;
}
