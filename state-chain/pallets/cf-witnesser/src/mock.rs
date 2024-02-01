#![cfg(test)]

use crate::{self as pallet_cf_witness, WitnessDataExtraction};
use cf_traits::{
	impl_mock_chainflip, impl_mock_runtime_safe_mode, AccountRoleRegistry, CallDispatchFilter,
};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{pallet_prelude::RuntimeDebug, parameter_types};
use frame_system as system;
use scale_info::TypeInfo;
use sp_core::H256;
use sp_runtime::traits::{BlakeTwo256, IdentityLookup};
use sp_std::collections::btree_set::BTreeSet;

type AccountId = u64;

pub mod dummy;
type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Witnesser: pallet_cf_witness,
		Dummy: dummy,
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
}

impl system::Config for Test {
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

impl_mock_runtime_safe_mode! { witnesser: pallet_cf_witness::PalletSafeMode<MockCallFilter> }

parameter_types! {
	pub static AllowCall: bool = true;
}

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Copy, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct MockCallFilter;

impl CallDispatchFilter<RuntimeCall> for MockCallFilter {
	fn should_dispatch(&self, _call: &RuntimeCall) -> bool {
		AllowCall::get()
	}
}

impl pallet_cf_witness::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type SafeMode = MockRuntimeSafeMode;
	type CallDispatchPermission = MockCallFilter;
	type WeightInfo = ();
}

impl_mock_chainflip!(Test);

impl dummy::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type EnsureWitnessed = pallet_cf_witness::EnsureWitnessed;
}

impl WitnessDataExtraction for RuntimeCall {
	fn extract(&mut self) -> Option<Vec<u8>> {
		None
	}

	fn combine_and_inject(&mut self, _data: &mut [Vec<u8>]) {
		// Do nothing
	}
}

pub const ALISSA: <Test as frame_system::Config>::AccountId = 1u64;
pub const BOBSON: <Test as frame_system::Config>::AccountId = 2u64;
pub const CHARLEMAGNE: <Test as frame_system::Config>::AccountId = 3u64;
pub const DEIRDRE: <Test as frame_system::Config>::AccountId = 4u64;
const GENESIS_AUTHORITIES: [u64; 3] = [ALISSA, BOBSON, CHARLEMAGNE];

cf_test_utilities::impl_test_helpers! {
	Test,
	RuntimeGenesisConfig::default(),
	||{
		MockEpochInfo::next_epoch(BTreeSet::from(GENESIS_AUTHORITIES));
		for id in GENESIS_AUTHORITIES.iter().chain(&[DEIRDRE]) {
			<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_validator(id)
				.unwrap();
		}
	}
}
