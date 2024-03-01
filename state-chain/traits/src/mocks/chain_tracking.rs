use cf_chains::Chain;

use crate::GetBlockHeight;

use super::{
	block_height_provider::BlockHeightProvider, tracked_data_provider::TrackedDataProvider,
};

pub type ChainTracking<T> = (BlockHeightProvider<T>, TrackedDataProvider<T>);

impl<C: Chain> GetBlockHeight<C> for ChainTracking<C> {
	fn get_block_height() -> C::ChainBlockNumber {
		BlockHeightProvider::<C>::get_block_height()
	}
}

// impl<C: Chain> GetTrackedData<C> for ChainTracking<C> {
// 	fn get_tracked_data() -> C::TrackedData {
// 		TrackedDataProvider::<C>::get_tracked_data()
// 	}
// }
