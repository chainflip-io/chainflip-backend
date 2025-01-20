#![cfg(test)]

use crate::{self as pallet_cf_account_roles, Config};
use cf_traits::mocks::deregistration_check::MockDeregistrationCheck;
use frame_support::derive_impl;

type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		MockAccountRoles: pallet_cf_account_roles,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type OnNewAccount = MockAccountRoles;
	type OnKilledAccount = MockAccountRoles;
}

impl Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type EnsureGovernance = frame_system::EnsureRoot<<Self as frame_system::Config>::AccountId>;
	type DeregistrationCheck = MockDeregistrationCheck<Self::AccountId>;
	type WeightInfo = ();
}

cf_test_utilities::impl_test_helpers!(Test);
