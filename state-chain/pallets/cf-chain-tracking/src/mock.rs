#![cfg(test)]

use std::cell::RefCell;

use crate::{self as pallet_cf_chain_tracking, Config};
use cf_chains::mocks::MockEthereum;
use cf_traits::{self, impl_mock_chainflip};
use frame_support::derive_impl;
type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		MockChainTracking: pallet_cf_chain_tracking,
	}
);

thread_local! {
	pub static NOMINATION: std::cell::RefCell<Option<u64>> = RefCell::new(Some(0xc001d00d_u64));
	pub static VALIDKEY: std::cell::RefCell<bool> = RefCell::new(true);
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

impl_mock_chainflip!(Test);

impl Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type TargetChain = MockEthereum;
	type WeightInfo = ();
}

cf_test_utilities::impl_test_helpers!(Test);
