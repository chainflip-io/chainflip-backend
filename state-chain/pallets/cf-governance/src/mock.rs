use std::cell::RefCell;

use crate::{self as pallet_cf_governance};
use cf_traits::{
	mocks::{epoch_info::MockEpochInfo, system_state_info::MockSystemStateInfo, time_source},
	Chainflip, ExecutionCondition, RuntimeUpgrade,
};
use frame_support::{dispatch::DispatchResultWithPostInfo, ensure, parameter_types};
use frame_system as system;
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;
type AccountId = u64;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
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

impl system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Index = u64;
	type BlockNumber = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = Header;
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
	pub fn set(mode: bool) {
		UPGRADE_SUCCEEDED.with(|cell| *cell.borrow_mut() = mode);
	}
}

cf_traits::impl_mock_ensure_witnessed_for_origin!(Origin);

impl Chainflip for Test {
	type KeyId = Vec<u8>;
	type ValidatorId = u64;
	type Amount = u128;
	type RuntimeCall = RuntimeCall;
	type EnsureWitnessed = MockEnsureWitnessed;
	type EnsureWitnessedAtCurrentEpoch = MockEnsureWitnessed;
	type EpochInfo = MockEpochInfo;
	type SystemState = MockSystemStateInfo;
}

impl pallet_cf_governance::Config for Test {
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
	type TimeSource = time_source::Mock;
	type EnsureGovernance = pallet_cf_governance::EnsureGovernance;
	type WeightInfo = ();
	type UpgradeCondition = UpgradeConditionMock;
	type RuntimeUpgrade = RuntimeUpgradeMock;
}

pub const ALICE: <Test as frame_system::Config>::AccountId = 123u64;
pub const BOB: <Test as frame_system::Config>::AccountId = 456u64;
pub const CHARLES: <Test as frame_system::Config>::AccountId = 789u64;
pub const EVE: <Test as frame_system::Config>::AccountId = 987u64;
pub const PETER: <Test as frame_system::Config>::AccountId = 988u64;
pub const MAX: <Test as frame_system::Config>::AccountId = 989u64;

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let config = GenesisConfig {
		system: Default::default(),
		governance: GovernanceConfig { members: vec![ALICE, BOB, CHARLES], expiry_span: 50 },
	};

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		// This is required to log events.
		System::set_block_number(1);
	});

	ext
}
