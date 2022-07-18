use super::*;
use crate as pallet_cf_validator;
use cf_traits::{
	mocks::{
		ensure_origin_mock::NeverFailingOriginCheck, epoch_info::MockEpochInfo,
		qualify_node::QualifyAll, reputation_resetter::MockReputationResetter,
		system_state_info::MockSystemStateInfo, vault_rotation::MockVaultRotator,
	},
	BackupNodes, Bid, Chainflip, ChainflipAccountData, QualifyNode, RuntimeAuctionOutcome,
};
use frame_support::{
	construct_runtime, parameter_types,
	traits::{OnInitialize, ValidatorRegistration},
};
use log::LevelFilter;
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
		System: frame_system,
		ValidatorPallet: pallet_cf_validator,
		Session: pallet_session,
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
	type MaxConsumers = frame_support::traits::ConstU32<5>;
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
	type NextSessionRotation = ();
	type WeightInfo = ();
}

pub struct MockAuctioneer;

pub const AUCTION_WINNERS: [ValidatorId; 5] = [0, 1, 2, 3, 4];
pub const WINNING_BIDS: [Amount; 5] = [120, 120, 110, 105, 100];
pub const AUCTION_LOSERS: [ValidatorId; 3] = [5, 6, 7];
pub const UNQUALIFIED_NODE: ValidatorId = 8;
pub const UNQUALIFIED_NODE_BID: Amount = 200;
pub const LOSING_BIDS: [Amount; 3] = [99, 90, 74];
pub const BOND: Amount = 100;

thread_local! {
	pub static NEXT_AUCTION_OUTCOME: RefCell<Result<RuntimeAuctionOutcome<Test>, &'static str>> = RefCell::new(Ok(
		RuntimeAuctionOutcome::<Test> {
			winners: AUCTION_WINNERS.to_vec(),
			losers: AUCTION_LOSERS.zip(LOSING_BIDS).map(Into::into).to_vec(),
			bond: BOND,
		}
	));
}

impl MockAuctioneer {
	pub fn set_next_auction_outcome(behaviour: Result<RuntimeAuctionOutcome<Test>, &'static str>) {
		NEXT_AUCTION_OUTCOME.with(|cell| {
			*cell.borrow_mut() = behaviour;
		});
	}
}

impl Auctioneer<Test> for MockAuctioneer {
	type Error = &'static str;

	fn resolve_auction() -> Result<RuntimeAuctionOutcome<Test>, Self::Error> {
		NEXT_AUCTION_OUTCOME.with(|cell| cell.borrow().clone())
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

	fn on_new_epoch(_epoch_authorities: &[Self::ValidatorId]) {
		// Reset the auctioneer to ensure we don't accidentally call it after the epoch has ended.
		MockAuctioneer::set_next_auction_outcome(Err("MockAuctioneer is not initialised"));
	}
}

pub struct MockQualifyValidator;
impl QualifyNode for MockQualifyValidator {
	type ValidatorId = ValidatorId;

	fn is_qualified(_validator_id: &Self::ValidatorId) -> bool {
		true
	}
}

thread_local! {
	pub static MISSED_SLOTS: RefCell<(u64, u64)> = RefCell::new(Default::default());
}

pub struct MockMissedAuthorshipSlots;

impl MockMissedAuthorshipSlots {
	pub fn set(expected: u64, authored: u64) {
		MISSED_SLOTS.with(|cell| *cell.borrow_mut() = (expected, authored))
	}

	pub fn get() -> (u64, u64) {
		MISSED_SLOTS.with(|cell| *cell.borrow())
	}
}

impl MissedAuthorshipSlots for MockMissedAuthorshipSlots {
	fn missed_slots() -> sp_std::ops::Range<u64> {
		let (expected, authored) = Self::get();
		expected..authored
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
	type EnsureWitnessed = NeverFailingOriginCheck<Self>;
	type EnsureWitnessedAtCurrentEpoch = NeverFailingOriginCheck<Self>;
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

pub struct MockBidderProvider;

impl BidderProvider for MockBidderProvider {
	type ValidatorId = ValidatorId;
	type Amount = Amount;

	fn get_bidders() -> Vec<Bid<Self::ValidatorId, Self::Amount>> {
		AUCTION_WINNERS
			.zip(WINNING_BIDS)
			.into_iter()
			.chain(AUCTION_LOSERS.zip(LOSING_BIDS))
			.chain(sp_std::iter::once((UNQUALIFIED_NODE, UNQUALIFIED_NODE_BID)))
			.map(|(bidder_id, amount)| Bid { bidder_id, amount })
			.collect()
	}
}

impl Config for Test {
	type Event = Event;
	type Offence = PalletOffence;
	type EpochTransitionHandler = TestEpochTransitionHandler;
	type MinEpoch = MinEpoch;
	type ValidatorWeightInfo = ();
	type Auctioneer = MockAuctioneer;
	type VaultRotator = MockVaultRotator;
	type ChainflipAccount = ChainflipAccountStore<Self>;
	type EnsureGovernance = NeverFailingOriginCheck<Self>;
	type MissedAuthorshipSlots = MockMissedAuthorshipSlots;
	type BidderProvider = MockBidderProvider;
	type OffenceReporter = MockOffenceReporter;
	type EmergencyRotationPercentageRange = EmergencyRotationPercentageRange;
	type Bonder = MockBonder;
	type ReputationResetter = MockReputationResetter<Self>;
	type ValidatorQualification = QualifyAll<ValidatorId>;
}

/// Session pallet requires a set of validators at genesis.
pub const GENESIS_VALIDATORS: [u64; 3] = [u64::MAX, u64::MAX - 1, u64::MAX - 2];
pub const CLAIM_PERCENTAGE_AT_GENESIS: Percentage = 50;
pub const GENESIS_BOND: Amount = 1;
pub const EPOCH_DURATION: u64 = 10;
pub(crate) struct TestExternalitiesWithCheck {
	ext: sp_io::TestExternalities,
}

#[macro_export]
macro_rules! assert_invariants {
	() => {
		assert_eq!(
			<ValidatorPallet as EpochInfo>::current_authorities(),
			Session::validators(),
			"Authorities out of sync at block {:?}. RotationStatus: {:?}",
			System::block_number(),
			ValidatorPallet::current_rotation_phase(),
		);
		let current_authorities =
			ValidatorPallet::current_authorities().into_iter().collect::<BTreeSet<_>>();
		let current_backups = ValidatorPallet::backup_nodes().into_iter().collect::<BTreeSet<_>>();
		assert!(
			current_authorities.is_disjoint(&current_backups),
			"Backup nodes and validators should not overlap",
		);
		assert!(
			ValidatorPallet::current_authorities()
				.iter()
				.all(|id| <Test as Config>::ChainflipAccount::get(id).state.is_authority()),
			"All authorities should have their account state set accordingly. Got: {:?}",
			ValidatorPallet::current_authorities()
				.iter()
				.map(|id| (id, <Test as Config>::ChainflipAccount::get(id).state))
				.collect::<Vec<_>>(),
		);
		assert!(
			ValidatorPallet::backup_nodes()
				.iter()
				.all(|id| <Test as Config>::ChainflipAccount::get(id).state.is_backup()),
			"All backup nodes should have their account state set accordingly. Got: {:?}",
			ValidatorPallet::backup_nodes()
				.iter()
				.map(|id| (id, <Test as Config>::ChainflipAccount::get(id).state))
				.collect::<Vec<_>>(),
		);
		assert!(
			BackupValidatorTriage::<Test>::get()
				.passive_nodes()
				.all(|id| <Test as Config>::ChainflipAccount::get(id).state.is_passive()),
			"All passive nodes should have their account state set accordingly. Got: {:?}",
			BackupValidatorTriage::<Test>::get()
				.passive_nodes()
				.map(|id| (id, <Test as Config>::ChainflipAccount::get(id).state))
				.collect::<Vec<_>>(),
		);
	};
}

impl TestExternalitiesWithCheck {
	pub fn execute_with<R>(&mut self, execute: impl FnOnce() -> R) -> R {
		self.ext.execute_with(|| {
			System::set_block_number(1);
			QualifyAll::<u64>::except(UNQUALIFIED_NODE);
			log::debug!("Pre-test invariant check.");
			assert_invariants!();
			log::debug!("Pre-test invariant check passed.");
			let r = execute();
			log::debug!("Post-test invariant check.");
			assert_invariants!();
			r
		})
	}

	pub fn execute_with_unchecked_invariants<R>(&mut self, execute: impl FnOnce() -> R) -> R {
		self.ext.execute_with(|| {
			System::set_block_number(1);
			QualifyAll::<u64>::except(UNQUALIFIED_NODE);
			execute()
		})
	}
}

pub(crate) fn new_test_ext() -> TestExternalitiesWithCheck {
	// Log nothing by default, set RUST_LOG=debug in the environment and use
	// `cargo test -- --no-capture` to enable debug logs.
	let _ = simple_logger::SimpleLogger::new()
		.with_level(LevelFilter::Off)
		.without_timestamps()
		.env()
		.init();

	log::debug!("Initializing test GenesisConfig.");
	let config = GenesisConfig {
		system: SystemConfig::default(),
		session: SessionConfig {
			keys: [&GENESIS_VALIDATORS[..], &AUCTION_WINNERS[..], &AUCTION_LOSERS[..]]
				.concat()
				.iter()
				.map(|&i| (i, i, UintAuthorityId(i).into()))
				.collect(),
		},
		validator_pallet: ValidatorPalletConfig {
			genesis_authorities: GENESIS_VALIDATORS.to_vec(),
			blocks_per_epoch: EPOCH_DURATION,
			bond: GENESIS_BOND,
			claim_period_as_percentage: CLAIM_PERCENTAGE_AT_GENESIS,
			backup_node_percentage: 34,
			authority_set_min_size: 3,
		},
	};

	let ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	TestExternalitiesWithCheck { ext }
}

pub fn run_to_block(n: u64) {
	assert_invariants!();
	while System::block_number() < n {
		log::debug!("Test::on_initialise({:?})", System::block_number());
		System::set_block_number(System::block_number() + 1);
		AllPalletsWithoutSystem::on_initialize(System::block_number());
		MockVaultRotator::on_initialise();
		assert_invariants!();
	}
}

pub fn move_forward_blocks(n: u64) {
	run_to_block(System::block_number() + n);
}
