use core::marker::PhantomData;

use cf_chains::Chain;

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

// impl<C: Chain> GetTrackedData<C> for TrackedDataProvider<C> {
// 	fn get_tracked_data() -> C::TrackedData {
// 		Self::get_value(TRACKED_DATA_KEY).expect("TrackedData must be set explicitly in mocks.")
// 	}
// }
