use super::*;
use crate as pallet_cf_validator;
use frame_support::{
	construct_runtime, parameter_types,
	traits::{OnFinalize, OnInitialize, ValidatorRegistration},
};

use cf_traits::{
	mocks::chainflip_account::MockChainflipAccount, ActiveValidatorRange, AuctionError,
	AuctionIndex, AuctionResult, Bid, BidderProvider, ChainflipAccount, ChainflipAccountData,
	IsOnline, IsOutgoing,
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

pub type Amount = u64;
pub type ValidatorId = u64;

pub const MIN_VALIDATOR_SIZE: u32 = 1;
pub const MAX_VALIDATOR_SIZE: u32 = 3;

thread_local! {
	pub static OLD_VALIDATORS: RefCell<Vec<u64>> = RefCell::new(vec![]);
	pub static CURRENT_VALIDATORS: RefCell<Vec<u64>> = RefCell::new(vec![]);
	pub static MIN_BID: RefCell<u64> = RefCell::new(0);
	pub static BIDDERS: RefCell<Vec<(u64, u64)>> = RefCell::new(vec![]);
}

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
type AuctionBehaviour = Vec<AuctionPhase<ValidatorId, Amount>>;

thread_local! {
	pub static AUCTION_INDEX: RefCell<AuctionIndex> = RefCell::new(0);
	pub static AUCTION_PHASE: RefCell<AuctionPhase<ValidatorId, Amount>> = RefCell::new(AuctionPhase::default());
	pub static AUCTION_BEHAVIOUR: RefCell<AuctionBehaviour> = RefCell::new(vec![]);
	pub static AUCTION_RESULT: RefCell<AuctionResult<ValidatorId, Amount>> = RefCell::new(AuctionResult {minimum_active_bid: 0, winners: vec![] });
}

pub enum AuctionScenario {
	HappyPath,
	Error,
}

impl MockAuctioneer {
	pub fn create_auction_scenario(
		bond: Amount,
		candidates: &[ValidatorId],
		scenario: AuctionScenario,
	) {
		MockBidderProvider::set_bidders(
			candidates
				.iter()
				.map(|validator_id| (*validator_id, bond + *validator_id))
				.collect(),
		);

		MockAuctioneer::set_phase(AuctionPhase::default());

		let behaviour =
			MockAuctioneer::create_behaviour(MockBidderProvider::get_bidders(), bond, scenario);
		MockAuctioneer::set_behaviour(behaviour);
	}

	pub fn create_behaviour(
		bidders: Vec<Bid<ValidatorId, Amount>>,
		bond: Amount,
		scenario: AuctionScenario,
	) -> AuctionBehaviour {
		match scenario {
			AuctionScenario::HappyPath => {
				vec![
					AuctionPhase::BidsTaken(bidders.clone()),
					AuctionPhase::ValidatorsSelected(
						bidders.iter().map(|(validator_id, _)| validator_id.clone()).collect(),
						bond,
					),
					AuctionPhase::ConfirmedValidators(
						bidders.iter().map(|(validator_id, _)| validator_id.clone()).collect(),
						bond,
					),
				]
			},
			AuctionScenario::Error => vec![AuctionPhase::BidsTaken(bidders.clone())],
		}
	}

	pub fn set_auction_result(result: AuctionResult<ValidatorId, Amount>) {
		AUCTION_RESULT.with(|cell| *cell.borrow_mut() = result);
	}

	pub fn set_phase(phase: AuctionPhase<ValidatorId, Amount>) {
		AUCTION_PHASE.with(|cell| *cell.borrow_mut() = phase);
	}

	pub fn set_behaviour(behaviour: AuctionBehaviour) {
		AUCTION_BEHAVIOUR.with(|cell| {
			*cell.borrow_mut() = behaviour;
			(*cell.borrow_mut()).reverse();
		});
	}

	pub fn next_auction() {
		AUCTION_INDEX.with(|cell| {
			let mut current_auction = cell.borrow_mut();
			*current_auction = *current_auction + 1;
		});
	}
}

impl Auctioneer for MockAuctioneer {
	type ValidatorId = ValidatorId;
	type Amount = Amount;
	type BidderProvider = MockBidderProvider;

	fn auction_index() -> AuctionIndex {
		AUCTION_INDEX.with(|cell| *(*cell).borrow())
	}

	fn active_range() -> ActiveValidatorRange {
		(MIN_VALIDATOR_SIZE, MAX_VALIDATOR_SIZE)
	}

	fn set_active_range(range: ActiveValidatorRange) -> Result<ActiveValidatorRange, AuctionError> {
		Ok(range)
	}

	fn auction_result() -> Option<AuctionResult<Self::ValidatorId, Self::Amount>> {
		AUCTION_RESULT.with(|cell| Some((*cell.borrow()).clone()))
	}

	fn phase() -> AuctionPhase<Self::ValidatorId, Self::Amount> {
		AUCTION_PHASE.with(|cell| (*cell.borrow()).clone())
	}

	fn waiting_on_bids() -> bool {
		Self::phase() == AuctionPhase::WaitingForBids
	}

	fn process() -> Result<AuctionPhase<Self::ValidatorId, Self::Amount>, AuctionError> {
		AUCTION_BEHAVIOUR.with(|cell| {
			let next_phase = (*cell.borrow_mut()).pop().ok_or(AuctionError::Abort)?;
			MockAuctioneer::set_phase(next_phase.clone());
			Ok(next_phase)
		})
	}

	fn abort() {
		MockAuctioneer::next_auction();
		MockAuctioneer::set_phase(AuctionPhase::default())
	}
}

pub struct MockIsOutgoing;
impl IsOutgoing for MockIsOutgoing {
	type AccountId = u64;

	fn is_outgoing(account_id: &Self::AccountId) -> bool {
		if let Some(last_active_epoch) = MockChainflipAccount::get(account_id).last_active_epoch {
			let current_epoch_index = ValidatorPallet::epoch_index();
			return last_active_epoch.saturating_add(1) == current_epoch_index
		}
		false
	}
}

pub struct MockOnline;
impl IsOnline for MockOnline {
	type ValidatorId = ValidatorId;

	fn is_online(_validator_id: &Self::ValidatorId) -> bool {
		true
	}
}

pub struct MockPeerMapping;
impl HasPeerMapping for MockPeerMapping {
	type ValidatorId = ValidatorId;

	fn has_peer_mapping(_validator_id: &Self::ValidatorId) -> bool {
		true
	}
}

impl ValidatorRegistration<ValidatorId> for Test {
	fn is_registered(_id: &ValidatorId) -> bool {
		true
	}
}

pub struct MockBidderProvider;
impl MockBidderProvider {
	pub fn set_bidders(bidders: Vec<Bid<ValidatorId, Amount>>) {
		BIDDERS.with(|cell| (*cell.borrow_mut()) = bidders)
	}
}

impl BidderProvider for MockBidderProvider {
	type ValidatorId = ValidatorId;
	type Amount = Amount;

	fn get_bidders() -> Vec<Bid<Self::ValidatorId, Self::Amount>> {
		BIDDERS.with(|cell| (*cell.borrow()).clone())
	}
}

pub struct TestEpochTransitionHandler;

impl EpochTransitionHandler for TestEpochTransitionHandler {
	type ValidatorId = ValidatorId;
	type Amount = Amount;

	fn on_new_epoch(
		old_validators: &[Self::ValidatorId],
		new_validators: &[Self::ValidatorId],
		new_bond: Self::Amount,
	) {
		OLD_VALIDATORS.with(|l| *l.borrow_mut() = old_validators.to_vec());
		CURRENT_VALIDATORS.with(|l| *l.borrow_mut() = new_validators.to_vec());
		MIN_BID.with(|l| *l.borrow_mut() = new_bond);

		for validator in new_validators {
			MockChainflipAccount::update_last_active_epoch(
				&validator,
				ValidatorPallet::epoch_index(),
			);
		}
	}
}

parameter_types! {
	pub const MinEpoch: u64 = 1;
	pub const MinValidatorSetSize: u32 = 2;
	pub const EmergencyRotationPercentageRange: PercentageRange = PercentageRange {
		bottom: 67,
		top: 80,
	};
}

impl Config for Test {
	type Event = Event;
	type MinEpoch = MinEpoch;
	type EpochTransitionHandler = TestEpochTransitionHandler;
	type ValidatorWeightInfo = ();
	type Amount = Amount;
	type Auctioneer = MockAuctioneer;
	type EmergencyRotationPercentageRange = EmergencyRotationPercentageRange;
}

/// Session pallet requires a set of validators at genesis.
pub const DUMMY_GENESIS_VALIDATORS: &'static [u64] = &[u64::MAX];

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
	// Initialise the auctioneer with an auction result
	MockAuctioneer::set_auction_result(AuctionResult {
		winners: DUMMY_GENESIS_VALIDATORS.to_vec(),
		minimum_active_bid: 0,
	});

	let config = GenesisConfig {
		system: SystemConfig::default(),
		session: SessionConfig {
			keys: DUMMY_GENESIS_VALIDATORS
				.iter()
				.map(|&i| (i, i, UintAuthorityId(i).into()))
				.collect(),
		},
		validator_pallet: ValidatorPalletConfig { blocks_per_epoch: 0 },
	};

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}

pub fn current_validators() -> Vec<u64> {
	CURRENT_VALIDATORS.with(|l| l.borrow().to_vec())
}

pub fn old_validators() -> Vec<u64> {
	OLD_VALIDATORS.with(|l| l.borrow().to_vec())
}

pub fn outgoing_validators() -> Vec<u64> {
	old_validators()
		.iter()
		.filter(|old_validator| !current_validators().contains(old_validator))
		.cloned()
		.collect()
}

pub fn min_bid() -> u64 {
	MIN_BID.with(|l| *l.borrow())
}

pub fn run_to_block(n: u64) {
	while System::block_number() < n {
		Session::on_finalize(System::block_number());
		System::set_block_number(System::block_number() + 1);
		Session::on_initialize(System::block_number());
	}
}

pub fn move_forward_blocks(n: u64) {
	run_to_block(System::block_number() + n);
}
