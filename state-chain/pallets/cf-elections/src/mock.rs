#![cfg(test)]

pub use crate::{self as pallet_cf_elections};

use crate::{electoral_systems, GenesisConfig as ElectionGenesisConfig};
use cf_traits::{impl_mock_chainflip, AccountRoleRegistry};
use frame_support::{
	derive_impl,
	instances::Instance1,
	sp_runtime::traits::{BlakeTwo256, IdentityLookup},
};
use sp_core::H256;

type AccountId = u64;
type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Elections: pallet_cf_elections::<Instance1>,
	}
);

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
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = frame_support::traits::ConstU64<250>;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = frame_support::traits::ConstU16<2112>;
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<5>;
}

impl pallet_cf_elections::Config<Instance1> for Test {
	type RuntimeEvent = RuntimeEvent;

	// Use the median electoral system as a simple way to test the election pallet
	// TODO: Use Settings?
	type ElectoralSystem = electoral_systems::median::UnsafeMedian<u64, (), ()>;
}

impl_mock_chainflip!(Test);

pub const INITIAL_UNSYNCED_STATE: u64 = 44;

cf_test_utilities::impl_test_helpers! {
	Test,
	RuntimeGenesisConfig {
		system: Default::default(),
		elections: ElectionGenesisConfig {
			option_initialize: Some((INITIAL_UNSYNCED_STATE, (), ()))
		}
	},
	|| {
		// We need valid validators to vote for things
		MockEpochInfo::next_epoch((0..3).collect());
		for id in &MockEpochInfo::current_authorities() {
			<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_validator(id).unwrap();
		}
	}
}
