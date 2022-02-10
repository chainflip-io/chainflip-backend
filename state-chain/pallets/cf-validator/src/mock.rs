use super::*;
use crate as pallet_cf_validator;
use frame_support::{
	construct_runtime, parameter_types,
	traits::{OnFinalize, OnInitialize, ValidatorRegistration},
};

use cf_traits::{
	mocks::{
		chainflip_account::MockChainflipAccount, ensure_origin_mock::NeverFailingOriginCheck,
		epoch_info::MockEpochInfo,
	},
	AuctionError, AuctionResult, Bid, BidderProvider, Chainflip, ChainflipAccount,
	ChainflipAccountData, IsOnline, IsOutgoing, QualifyValidator, VaultRotator,
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

thread_local! {
	pub static OLD_VALIDATORS: RefCell<Vec<u64>> = RefCell::new(vec![]);
	pub static CURRENT_VALIDATORS: RefCell<Vec<u64>> = RefCell::new(vec![]);
	pub static MIN_BID: RefCell<Amount> = RefCell::new(0);
	pub static BIDDERS: RefCell<Vec<(u64, Amount)>> = RefCell::new(vec![]);
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

thread_local! {
	pub static AUCTION_RUN_BEHAVIOUR: RefCell<Result<AuctionResult<ValidatorId, Amount>, AuctionError>> = RefCell::new(Ok(Default::default()));
}

impl MockAuctioneer {
	pub fn set_run_behaviour(behaviour: Result<AuctionResult<ValidatorId, Amount>, AuctionError>) {
		AUCTION_RUN_BEHAVIOUR.with(|cell| {
			*cell.borrow_mut() = behaviour;
		});
	}
}

impl Auctioneer for MockAuctioneer {
	type ValidatorId = ValidatorId;
	type Amount = Amount;

	fn resolve_auction() -> Result<AuctionResult<Self::ValidatorId, Self::Amount>, AuctionError> {
		AUCTION_RUN_BEHAVIOUR.with(|cell| match (*cell.borrow()).as_ref() {
			Ok(a) => Ok((*a).clone()),
			Err(e) => Err(*e),
		})
	}

	fn update_validator_status(_auction: AuctionResult<Self::ValidatorId, Self::Amount>) {}
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

impl ValidatorRegistration<ValidatorId> for Test {
	fn is_registered(_id: &ValidatorId) -> bool {
		true
	}
}

pub struct MockBidderProvider;

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

pub struct MockVaultRotator;

impl VaultRotator for MockVaultRotator {
	type ValidatorId = ValidatorId;
	type RotationError = AuctionError;

	/// Start a vault rotation with the following `candidates`
	fn start_vault_rotation(
		_candidates: Vec<Self::ValidatorId>,
	) -> Result<(), Self::RotationError> {
		Ok(())
	}

	/// In order for the validators to be rotated we are waiting on a confirmation that the vaults
	/// have been rotated.
	fn finalize_rotation() -> bool {
		true
	}
}

pub struct MockQualifyValidator;
impl QualifyValidator for MockQualifyValidator {
	type ValidatorId = ValidatorId;

	fn is_qualified(validator_id: &Self::ValidatorId) -> bool {
		MockOnline::is_online(validator_id)
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

impl Chainflip for Test {
	type KeyId = Vec<u8>;
	type ValidatorId = ValidatorId;
	type Amount = Amount;
	type Call = Call;
	type EnsureWitnessed = NeverFailingOriginCheck<Self>;
	type EpochInfo = MockEpochInfo;
}

impl Config for Test {
	type Event = Event;
	type MinEpoch = MinEpoch;
	type EpochTransitionHandler = TestEpochTransitionHandler;
	type ValidatorWeightInfo = ();
	type Auctioneer = MockAuctioneer;
	type EmergencyRotationPercentageRange = EmergencyRotationPercentageRange;
	type VaultRotator = MockVaultRotator;
	type ChainflipAccount = MockChainflipAccount;
}

/// Session pallet requires a set of validators at genesis.
pub const DUMMY_GENESIS_VALIDATORS: &'static [u64] = &[u64::MAX];
pub const MINIMUM_ACTIVE_BID_AT_GENESIS: Amount = 1;
pub const BLOCKS_TO_SESSION_ROTATION: u64 = 4;

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
	// Initialise the auctioneer with an auction result
	MockAuctioneer::set_run_behaviour(Ok(AuctionResult {
		winners: DUMMY_GENESIS_VALIDATORS.to_vec(),
		minimum_active_bid: MINIMUM_ACTIVE_BID_AT_GENESIS,
	}));

	let config = GenesisConfig {
		system: SystemConfig::default(),
		session: SessionConfig {
			keys: DUMMY_GENESIS_VALIDATORS
				.iter()
				.map(|&i| (i, i, UintAuthorityId(i).into()))
				.collect(),
		},
		validator_pallet: ValidatorPalletConfig {
			blocks_per_epoch: 0,
			bond: MINIMUM_ACTIVE_BID_AT_GENESIS,
		},
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

pub fn min_bid() -> Amount {
	MIN_BID.with(|l| *l.borrow())
}

pub fn run_to_block(n: u64) {
	while System::block_number() < n {
		Session::on_finalize(System::block_number());
		System::set_block_number(System::block_number() + 1);
		Session::on_initialize(System::block_number());
		<ValidatorPallet as OnInitialize<u64>>::on_initialize(System::block_number());
	}
}

pub fn move_forward_blocks(n: u64) {
	run_to_block(System::block_number() + n);
}
