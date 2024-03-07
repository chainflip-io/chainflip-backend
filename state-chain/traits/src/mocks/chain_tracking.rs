use super::MockPallet;
use crate::mocks::MockPalletStorage;
use cf_chains::Chain;

use crate::{AdjustedFeeEstimationApi, GetBlockHeight};

use super::{
	block_height_provider::BlockHeightProvider, tracked_data_provider::TrackedDataProvider,
};

pub struct ChainTracker<C: Chain>(BlockHeightProvider<C>, TrackedDataProvider<C>);

impl<C: Chain> MockPallet for ChainTracker<C> {
	const PREFIX: &'static [u8] = b"MockChainTrackerProvider";
}

const TRACKED_FEE_KEY: &[u8] = b"TRACKED_FEE_DATA";

impl<C: Chain> ChainTracker<C> {
	pub fn set_fee(fee: C::ChainAmount) {
		Self::put_value(TRACKED_FEE_KEY, fee);
	}
}

impl<C: Chain> GetBlockHeight<C> for ChainTracker<C> {
	fn get_block_height() -> C::ChainBlockNumber {
		BlockHeightProvider::<C>::get_block_height()
	}
}

impl<C: Chain> AdjustedFeeEstimationApi<C> for ChainTracker<C> {
	fn estimate_ingress_fee(_asset: C::ChainAsset) -> C::ChainAmount {
		Self::get_value(TRACKED_FEE_KEY).unwrap_or_default()
	}

	fn estimate_egress_fee(_asset: C::ChainAsset) -> C::ChainAmount {
		Self::get_value(TRACKED_FEE_KEY).unwrap_or_default()
	}
}
