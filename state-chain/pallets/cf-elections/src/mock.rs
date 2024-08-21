#![cfg(test)]

pub use crate::{self as pallet_cf_elections};

use crate::{electoral_systems, GenesisConfig as ElectionGenesisConfig};
use cf_traits::{impl_mock_chainflip, AccountRoleRegistry};
use frame_support::{derive_impl, instances::Instance1};

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Elections: pallet_cf_elections::<Instance1>,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

impl pallet_cf_elections::Config<Instance1> for Test {
	type RuntimeEvent = RuntimeEvent;

	// Use the median electoral system as a simple way to test the election pallet
	// TODO: Use Settings?
	type ElectoralSystem = electoral_systems::unsafe_median::UnsafeMedian<u64, (), ()>;
}

impl_mock_chainflip!(Test);

pub const INITIAL_UNSYNCED_STATE: u64 = 44;

cf_test_utilities::impl_test_helpers! {
	Test,
	RuntimeGenesisConfig {
		system: Default::default(),
		elections: ElectionGenesisConfig {
			option_initial_state: Some(crate::InitialState {
				unsynchronised_state: INITIAL_UNSYNCED_STATE,
				unsynchronised_settings: (),
				settings: ()
			})
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
