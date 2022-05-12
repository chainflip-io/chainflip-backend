use super::*;
use crate as pallet_cf_auction;
use cf_traits::{
	impl_mock_online,
	mocks::{
		chainflip_account::MockChainflipAccount, ensure_origin_mock::NeverFailingOriginCheck,
		epoch_info::MockEpochInfo, keygen_exclusion::MockKeygenExclusion,
		system_state_info::MockSystemStateInfo,
	},
	Chainflip, ChainflipAccountData, EmergencyRotation, IsOnline,
};
use frame_support::{construct_runtime, parameter_types, traits::ValidatorRegistration};
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};
use std::{cell::RefCell, collections::HashMap};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

pub type Amount = u128;
pub type ValidatorId = u64;

pub const MIN_AUTHORITY_SIZE: u32 = 1;
pub const MAX_AUTHORITY_SIZE: u32 = 3;
pub const MAX_AUTHORITY_SET_EXPANSION: u32 = 2;
pub const MAX_AUTHORITY_SET_CONTRACTION: u32 = 2;

thread_local! {
	// A set of bidders, we initialise this with the proposed genesis bidders
	pub static BIDDER_SET: RefCell<Vec<(ValidatorId, Amount)>> = RefCell::new(vec![]);
	pub static CHAINFLIP_ACCOUNTS: RefCell<HashMap<u64, ChainflipAccountData>> = RefCell::new(HashMap::new());
	pub static EMERGENCY_ROTATION: RefCell<bool> = RefCell::new(false);
}

construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
		AuctionPallet: pallet_cf_auction::{Pallet, Call, Storage, Event<T>, Config},
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
	type AccountId = u64;
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

pub struct MockEmergencyRotation;

impl EmergencyRotation for MockEmergencyRotation {
	fn request_emergency_rotation() {
		EMERGENCY_ROTATION.with(|cell| *cell.borrow_mut() = true);
	}

	fn emergency_rotation_in_progress() -> bool {
		EMERGENCY_ROTATION.with(|cell| *cell.borrow())
	}

	fn emergency_rotation_completed() {}
}

impl_mock_online!(ValidatorId);

pub struct MockQualifyValidator;
impl QualifyNode for MockQualifyValidator {
	type ValidatorId = ValidatorId;

	fn is_qualified(validator_id: &Self::ValidatorId) -> bool {
		MockOnline::is_online(validator_id)
	}
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

impl Config for Test {
	type Event = Event;
	type BidderProvider = MockBidderProvider;
	type ChainflipAccount = MockChainflipAccount;
	type KeygenExclusionSet = MockKeygenExclusion<Self>;
	type WeightInfo = ();
	type EmergencyRotation = MockEmergencyRotation;
	type AuctionQualification = MockQualifyValidator;
	type EnsureGovernance = NeverFailingOriginCheck<Self>;
}

impl ValidatorRegistration<ValidatorId> for Test {
	fn is_registered(_id: &ValidatorId) -> bool {
		true
	}
}

pub struct MockBidderProvider;

impl MockBidderProvider {
	// Create a set of descending bids, including an invalid bid of amount 0
	// offset the ids to create unique bidder groups.  By default all bidders are online.
	pub fn set_bids(bids: &[(ValidatorId, Amount)]) {
		BIDDER_SET.with(|cell| {
			*cell.borrow_mut() = bids.to_vec();
		});
	}
}

impl BidderProvider for MockBidderProvider {
	type ValidatorId = ValidatorId;
	type Amount = Amount;

	fn get_bidders() -> Vec<(Self::ValidatorId, Self::Amount)> {
		BIDDER_SET.with(|l| l.borrow().to_vec())
	}
}

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
	let config = GenesisConfig {
		system: Default::default(),
		auction_pallet: AuctionPalletConfig {
			min_size: MIN_AUTHORITY_SIZE,
			max_size: MAX_AUTHORITY_SIZE,
			max_expansion: MAX_AUTHORITY_SET_EXPANSION,
			max_contraction: MAX_AUTHORITY_SET_CONTRACTION,
		},
	};

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
