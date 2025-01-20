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
	fn estimate_ingress_fee(&self, asset: C::ChainAsset) -> C::ChainAmount {
		Self::get_value::<C::TrackedData>(TRACKED_DATA_KEY)
			.expect("TrackedData must be set explicitly in mocks")
			.estimate_ingress_fee(asset)
	}

	fn estimate_ingress_fee_vault_swap(&self) -> Option<<C as Chain>::ChainAmount> {
		Self::get_value::<C::TrackedData>(TRACKED_DATA_KEY)
			.expect("TrackedData must be set explicitly in mocks")
			.estimate_ingress_fee_vault_swap()
	}

	fn estimate_egress_fee(&self, asset: C::ChainAsset) -> C::ChainAmount {
		Self::get_value::<C::TrackedData>(TRACKED_DATA_KEY)
			.expect("TrackedData must be set explicitly in mocks")
			.estimate_egress_fee(asset)
	}

	fn estimate_ccm_fee(
		&self,
		asset: <C as Chain>::ChainAsset,
		gas_budget: cf_primitives::GasAmount,
		message_length: usize,
	) -> Option<<C as Chain>::ChainAmount> {
		Self::get_value::<C::TrackedData>(TRACKED_DATA_KEY)
			.expect("TrackedData must be set explicitly in mocks")
			.estimate_ccm_fee(asset, gas_budget, message_length)
	}
}
