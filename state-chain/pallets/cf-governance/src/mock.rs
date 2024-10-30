use std::cell::RefCell;

use crate::{self as pallet_cf_governance};
use cf_primitives::SemVer;
use cf_traits::{
	mocks::{
		account_role_registry::MockAccountRoleRegistry, ensure_origin_mock::FailOnNoneOrigin,
		funding_info::MockFundingInfo, time_source,
	},
	AuthoritiesCfeVersions, CompatibleCfeVersions, ExecutionCondition, RuntimeUpgrade,
};
use frame_support::{derive_impl, dispatch::DispatchResultWithPostInfo, ensure, parameter_types};
use frame_system as system;
use sp_runtime::Percent;
use sp_std::collections::btree_set::BTreeSet;

type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Governance: pallet_cf_governance,
	}
);

thread_local! {
	pub static UPGRADE_CONDITIONS_SATISFIED: std::cell::RefCell<bool>  = RefCell::new(true);
	pub static UPGRADE_SUCCEEDED: std::cell::RefCell<bool>  = RefCell::new(true);
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl system::Config for Test {
	type Block = Block;
}

cf_traits::impl_mock_epoch_info!(
	<Test as frame_system::Config>::AccountId,
	u128,
	cf_primitives::EpochIndex,
	cf_primitives::AuthorityCount,
);

impl cf_traits::Chainflip for Test {
	type Amount = u128;
	type ValidatorId = <Test as frame_system::Config>::AccountId;
	type RuntimeCall = RuntimeCall;
	type EnsureWitnessed = FailOnNoneOrigin<Self>;
	type EnsurePrewitnessed = FailOnNoneOrigin<Self>;
	type EnsureWitnessedAtCurrentEpoch = FailOnNoneOrigin<Self>;
	// Using actual EnsureGovernance instead of mock
	type EnsureGovernance = crate::EnsureGovernance;
	type EpochInfo = MockEpochInfo;
	type AccountRoleRegistry = MockAccountRoleRegistry;
	type FundingInfo = MockFundingInfo<Self>;
}

pub struct UpgradeConditionMock;

impl ExecutionCondition for UpgradeConditionMock {
	fn is_satisfied() -> bool {
		UPGRADE_CONDITIONS_SATISFIED.with(|cell| *cell.borrow())
	}
}

impl UpgradeConditionMock {
	pub fn set(mode: bool) {
		UPGRADE_CONDITIONS_SATISFIED.with(|cell| *cell.borrow_mut() = mode);
	}
}

pub struct RuntimeUpgradeMock;

impl RuntimeUpgrade for RuntimeUpgradeMock {
	fn do_upgrade(_: Vec<u8>) -> DispatchResultWithPostInfo {
		ensure!(
			UPGRADE_SUCCEEDED.with(|cell| *cell.borrow()),
			frame_system::Error::<Test>::FailedToExtractRuntimeVersion
		);
		Ok(().into())
	}
}

impl RuntimeUpgradeMock {
	pub fn upgrade_success(mode: bool) {
		UPGRADE_SUCCEEDED.with(|cell| *cell.borrow_mut() = mode);
	}
}

parameter_types! {
	pub static PercentCfeAtTargetVersion: Percent = Percent::from_percent(100);
}

pub struct MockAuthoritiesCfeVersions;
impl AuthoritiesCfeVersions for MockAuthoritiesCfeVersions {
	fn percent_authorities_compatible_with_version(_version: SemVer) -> Percent {
		PercentCfeAtTargetVersion::get()
	}
}

pub struct MockCompatibleCfeVersions;
impl CompatibleCfeVersions for MockCompatibleCfeVersions {
	fn current_release_version() -> SemVer {
		SemVer { major: 1, minor: 0, patch: 0 }
	}
}

impl pallet_cf_governance::Config for Test {
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
	type TimeSource = time_source::Mock;
	type WeightInfo = ();
	type UpgradeCondition = UpgradeConditionMock;
	type RuntimeUpgrade = RuntimeUpgradeMock;
	type AuthoritiesCfeVersions = MockAuthoritiesCfeVersions;
	type CompatibleCfeVersions = MockCompatibleCfeVersions;
}

pub const ALICE: <Test as frame_system::Config>::AccountId = 123u64;
pub const BOB: <Test as frame_system::Config>::AccountId = 456u64;
pub const CHARLES: <Test as frame_system::Config>::AccountId = 789u64;
pub const EVE: <Test as frame_system::Config>::AccountId = 987u64;
pub const PETER: <Test as frame_system::Config>::AccountId = 988u64;
pub const MAX: <Test as frame_system::Config>::AccountId = 989u64;

/// Not a member of the governance, used for testing ensure governance check
pub const NOT_GOV_MEMBER: <Test as frame_system::Config>::AccountId = 6969u64;

cf_test_utilities::impl_test_helpers! {
	Test,
	RuntimeGenesisConfig {
		system: Default::default(),
		governance: GovernanceConfig {
			members: BTreeSet::from([ALICE, BOB, CHARLES]),
			expiry_span: 50,
		},
	}
}
