use super::*;
use crate as pallet_cf_validator;
use frame_support::{
	construct_runtime, parameter_types,
	traits::{OnFinalize, OnInitialize, ValidatorRegistration},
};

use cf_traits::{
	mocks::{
		chainflip_account::MockChainflipAccount, ensure_origin_mock::NeverFailingOriginCheck,
		epoch_info::MockEpochInfo, reputation_resetter::MockReputationResetter,
		system_state_info::MockSystemStateInfo, vault_rotation::MockVaultRotator,
	},
	AuctionResult, Chainflip, ChainflipAccount, ChainflipAccountData, IsOnline, QualifyNode,
};
use sp_core::H256;
use sp_runtime::{
	impl_opaque_keys,
	testing::{Header, UintAuthorityId},
	traits::{BlakeTwo256, ConvertInto, IdentityLookup},
	BuildStorage, Perbill,
};
use std::cell::RefCell;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

pub type Amount = u128;
pub type ValidatorId = u64;

construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
		Session: pallet_session::{Pallet, Call, Storage, Event, Config<T>},
		ValidatorPallet: pallet_cf_validator::{Pallet, Call, Storage, Event<T>, Config<T>},
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
}
impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type Origin = Origin;
	type Call = Call;
	type Index = u64;
	type BlockNumber = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = ValidatorId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = Header;
	type Event = Event;
	type BlockHashCount = BlockHashCount;
	type DbWeight = ();
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ChainflipAccountData;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
}

impl_opaque_keys! {
	pub struct MockSessionKeys {
		pub dummy: UintAuthorityId,
	}
}

impl From<UintAuthorityId> for MockSessionKeys {
	fn from(dummy: UintAuthorityId) -> Self {
		Self { dummy }
	}
}

parameter_types! {
	pub const DisabledValidatorsThreshold: Perbill = Perbill::from_percent(33);
}

impl pallet_session::Config for Test {
	type ShouldEndSession = ValidatorPallet;
	type SessionManager = ValidatorPallet;
	type SessionHandler = pallet_session::TestSessionHandler;
	type ValidatorId = ValidatorId;
	type ValidatorIdOf = ConvertInto;
	type Keys = MockSessionKeys;
	type Event = Event;
	type DisabledValidatorsThreshold = DisabledValidatorsThreshold;
	type NextSessionRotation = ();
	type WeightInfo = ();
}

pub struct MockAuctioneer;

thread_local! {
	pub static AUCTION_RUN_BEHAVIOUR: RefCell<Result<AuctionResult<ValidatorId, Amount>, &'static str>> = RefCell::new(Ok(Default::default()));
	pub static AUCTION_WINNERS: RefCell<Option<Vec<ValidatorId>>> = RefCell::new(None);
}

impl MockAuctioneer {
	pub fn set_run_behaviour(behaviour: Result<AuctionResult<ValidatorId, Amount>, &'static str>) {
		AUCTION_RUN_BEHAVIOUR.with(|cell| {
			*cell.borrow_mut() = behaviour;
		});
	}
}

impl Auctioneer for MockAuctioneer {
	type ValidatorId = ValidatorId;
	type Amount = Amount;
	type Error = &'static str;

	fn resolve_auction() -> Result<AuctionResult<Self::ValidatorId, Self::Amount>, Self::Error> {
		AUCTION_RUN_BEHAVIOUR.with(|cell| {
			let run_behaviour = (*cell.borrow()).clone();
			run_behaviour.map(|result| {
				AUCTION_WINNERS.with(|cell| {
					*cell.borrow_mut() = Some(result.winners.to_vec());
				});
				result
			})
		})
	}

	fn update_backup_and_passive_states() {
		// no op
	}
}

pub struct MockOnline;
impl IsOnline for MockOnline {
	type ValidatorId = ValidatorId;

	fn is_online(_validator_id: &Self::ValidatorId) -> bool {
		true
	}
}

impl ValidatorRegistration<ValidatorId> for Test {
	fn is_registered(_id: &ValidatorId) -> bool {
		true
	}
}

pub struct TestEpochTransitionHandler;

impl EpochTransitionHandler for TestEpochTransitionHandler {
	type ValidatorId = ValidatorId;

	fn on_new_epoch(epoch_authorities: &[Self::ValidatorId]) {
		for authority in epoch_authorities {
			MockChainflipAccount::set_current_authority(authority);
		}
	}
}

pub struct MockQualifyValidator;
impl QualifyNode for MockQualifyValidator {
	type ValidatorId = ValidatorId;

	fn is_qualified(validator_id: &Self::ValidatorId) -> bool {
		MockOnline::is_online(validator_id)
	}
}

thread_local! {
	pub static MISSED_SLOTS: RefCell<Vec<u64>> = RefCell::new(Default::default());
}

pub struct MockMissedAuthorshipSlots;

impl MockMissedAuthorshipSlots {
	pub fn set(slots: Vec<u64>) {
		MISSED_SLOTS.with(|cell| *cell.borrow_mut() = slots)
	}

	pub fn get() -> Vec<u64> {
		MISSED_SLOTS.with(|cell| cell.borrow().clone())
	}
}

impl MissedAuthorshipSlots for MockMissedAuthorshipSlots {
	fn missed_slots() -> Vec<u64> {
		Self::get()
	}
}

parameter_types! {
	pub const MinEpoch: u64 = 1;
	pub const EmergencyRotationPercentageRange: PercentageRange = PercentageRange {
		bottom: 67,
		top: 80,
	};
}

impl Chainflip for Test {
	type KeyId = Vec<u8>;
	type ValidatorId = ValidatorId;
	type Amount = Amount;
	type Call = Call;
	type EnsureWitnessedByHistoricalActiveEpoch = NeverFailingOriginCheck<Self>;
	type EpochInfo = MockEpochInfo;
	type SystemState = MockSystemStateInfo;
}

pub struct MockBonder;

impl Bonding for MockBonder {
	type ValidatorId = ValidatorId;

	type Amount = Amount;

	fn update_bond(_: &Self::ValidatorId, _: Self::Amount) {}
}

pub type MockOffenceReporter =
	cf_traits::mocks::offence_reporting::MockOffenceReporter<ValidatorId, PalletOffence>;

impl Config for Test {
	type Event = Event;
	type Offence = PalletOffence;
	type MinEpoch = MinEpoch;
	type EpochTransitionHandler = TestEpochTransitionHandler;
	type ValidatorWeightInfo = ();
	type Auctioneer = MockAuctioneer;
	type EmergencyRotationPercentageRange = EmergencyRotationPercentageRange;
	type VaultRotator = MockVaultRotator;
	type ChainflipAccount = MockChainflipAccount;
	type EnsureGovernance = NeverFailingOriginCheck<Self>;
	type Bonder = MockBonder;
	type MissedAuthorshipSlots = MockMissedAuthorshipSlots;
	type OffenceReporter = MockOffenceReporter;
	type ReputationResetter = MockReputationResetter<Self>;
}

/// Session pallet requires a set of validators at genesis.
pub const DUMMY_GENESIS_VALIDATORS: &[u64] = &[u64::MAX];
pub const CLAIM_PERCENTAGE_AT_GENESIS: Percentage = 50;
pub const MINIMUM_ACTIVE_BID_AT_GENESIS: Amount = 1;
pub const EPOCH_DURATION: u64 = 10;

pub(crate) struct TestExternalitiesWithCheck {
	ext: sp_io::TestExternalities,
}

impl TestExternalitiesWithCheck {
	fn check_invariants() {
		assert_eq!(CurrentAuthorities::<Test>::get(), Session::validators(),);
	}

	pub fn execute_with<R>(&mut self, execute: impl FnOnce() -> R) -> R {
		self.ext.execute_with(|| {
			System::set_block_number(1);
			Self::check_invariants();
			let r = execute();
			Self::check_invariants();
			r
		})
	}

	pub fn execute_with_unchecked_invariants<R>(&mut self, execute: impl FnOnce() -> R) -> R {
		self.ext.execute_with(|| {
			System::set_block_number(1);
			execute()
		})
	}
}

pub(crate) fn new_test_ext() -> TestExternalitiesWithCheck {
	let config = GenesisConfig {
		system: SystemConfig::default(),
		session: SessionConfig {
			keys: DUMMY_GENESIS_VALIDATORS
				.iter()
				.map(|&i| (i, i, UintAuthorityId(i).into()))
				.collect(),
		},
		validator_pallet: ValidatorPalletConfig {
			blocks_per_epoch: EPOCH_DURATION,
			bond: MINIMUM_ACTIVE_BID_AT_GENESIS,
			claim_period_as_percentage: CLAIM_PERCENTAGE_AT_GENESIS,
		},
	};

	let ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	TestExternalitiesWithCheck { ext }
}

pub fn run_to_block(n: u64) {
	assert_eq!(<ValidatorPallet as EpochInfo>::current_authorities(), Session::validators());
	while System::block_number() < n {
		Session::on_finalize(System::block_number());
		System::set_block_number(System::block_number() + 1);
		Session::on_initialize(System::block_number());
		<ValidatorPallet as OnInitialize<u64>>::on_initialize(System::block_number());
		MockVaultRotator::on_initialise();
		assert_eq!(<ValidatorPallet as EpochInfo>::current_authorities(), Session::validators());
	}
}

pub fn move_forward_blocks(n: u64) {
	run_to_block(System::block_number() + n);
}
