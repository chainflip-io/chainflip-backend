use core::marker::PhantomData;

use cf_chains::{Chain, FeeEstimationApi};

use super::MockPallet;
use crate::mocks::MockPalletStorage;

pub struct TrackedDataProvider<C: Chain>(PhantomData<C>);

impl<C: Chain> MockPallet for TrackedDataProvider<C> {
	const PREFIX: &'static [u8] = b"MockTrackedDataProvider";
}

const TRACKED_DATA_KEY: &[u8] = b"TRACKED_DATA";

impl<C: Chain> TrackedDataProvider<C> {
	pub fn set_tracked_data(height: C::TrackedData) {
		Self::put_value(TRACKED_DATA_KEY, height);
	}
}

impl<C: Chain> FeeEstimationApi<C> for TrackedDataProvider<C> {
	fn estimate_ingress_fee(&self, _asset: C::ChainAsset) -> C::ChainAmount {
		Self::get_value(TRACKED_DATA_KEY).expect("TrackedData must be set explicitly in mocks")
	}

	fn estimate_egress_fee(&self, _asset: C::ChainAsset) -> C::ChainAmount {
		Self::get_value(TRACKED_DATA_KEY).expect("TrackedData must be set explicitly in mocks")
	}
}
