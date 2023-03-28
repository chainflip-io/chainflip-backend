use crate::{address::ForeignChainAddress, Chain, IngressIdConstructor};
use cf_primitives::{
	chains::{assets, AnyChain},
	AssetAmount,
};

impl Chain for AnyChain {
	type ChainBlockNumber = u64;
	type ChainAmount = AssetAmount;
	type TransactionFee = Self::ChainAmount;
	type TrackedData = ();
	type ChainAsset = assets::any::Asset;
	type ChainAccount = ForeignChainAddress;
	type EpochStartData = ();
	type IngressFetchId = ();
}

impl IngressIdConstructor for () {
	type Address = ForeignChainAddress;

	fn deployed(_intent_id: u64, _address: Self::Address) -> Self {
		unreachable!()
	}

	fn undeployed(_intent_id: u64, _address: Self::Address) -> Self {
		unreachable!()
	}
}
