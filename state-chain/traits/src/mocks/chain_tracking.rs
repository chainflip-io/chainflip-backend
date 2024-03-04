use cf_chains::Chain;

use crate::{FeeCalculationApi, GetBlockHeight};

use super::{
	block_height_provider::BlockHeightProvider, tracked_data_provider::TrackedDataProvider,
};

pub type ChainTracking<T> = (BlockHeightProvider<T>, TrackedDataProvider<T>);

impl<C: Chain> GetBlockHeight<C> for ChainTracking<C> {
	fn get_block_height() -> C::ChainBlockNumber {
		BlockHeightProvider::<C>::get_block_height()
	}
}

impl<C: Chain> FeeCalculationApi<C> for ChainTracking<C> {
	fn estimate_ingress_fee(_asset: C::ChainAsset) -> C::ChainAmount {
		// TrackedDataProvider::<C>::estimate_ingress_fee(_asset)
		Default::default()
	}

	fn estimate_egress_fee(_asset: C::ChainAsset) -> C::ChainAmount {
		// TrackedDataProvider::<C>::estimate_egress_fee(_asset)
		Default::default()
	}
}
