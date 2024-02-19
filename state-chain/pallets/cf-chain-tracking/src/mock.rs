#![cfg(test)]

use std::cell::RefCell;

use crate::{self as pallet_cf_chain_tracking, Config};
use cf_chains::mocks::MockEthereum;
use cf_traits::{self, impl_mock_chainflip};
use frame_support::{derive_impl, parameter_types};
use sp_core::H256;
use sp_runtime::traits::{BlakeTwo256, IdentityLookup};
type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		MockChainTracking: pallet_cf_chain_tracking,
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
}

thread_local! {
	pub static NOMINATION: std::cell::RefCell<Option<u64>> = RefCell::new(Some(0xc001d00d_u64));
	pub static VALIDKEY: std::cell::RefCell<bool> = RefCell::new(true);
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Nonce = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = BlockHashCount;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = SS58Prefix;
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<5>;
}

impl_mock_chainflip!(Test);

impl Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type TargetChain = MockEthereum;
	type WeightInfo = ();
}

cf_test_utilities::impl_test_helpers!(Test);
