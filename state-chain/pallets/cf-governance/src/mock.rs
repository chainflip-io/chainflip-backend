use std::cell::RefCell;

use crate::{self as pallet_cf_governance};
use cf_primitives::SemVer;
use cf_traits::{
	impl_mock_chainflip, mocks::time_source, AuthoritiesCfeVersions, CompatibleCfeVersions,
	ExecutionCondition, RuntimeUpgrade,
};
use frame_support::{derive_impl, dispatch::DispatchResultWithPostInfo, ensure, parameter_types};
use frame_system as system;
use sp_core::H256;
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	Percent,
};
use sp_std::collections::btree_set::BTreeSet;

type AccountId = u64;
type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Governance: pallet_cf_governance,
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
}

thread_local! {
	pub static UPGRADE_CONDITIONS_SATISFIED: std::cell::RefCell<bool>  = RefCell::new(true);
	pub static UPGRADE_SUCCEEDED: std::cell::RefCell<bool>  = RefCell::new(true);
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
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

impl_mock_chainflip!(Test);

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

cf_traits::impl_mock_ensure_witnessed_for_origin!(RuntimeOrigin);

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
