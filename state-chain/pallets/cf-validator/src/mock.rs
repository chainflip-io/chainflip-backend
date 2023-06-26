#![cfg(test)]

use super::*;
use crate as pallet_cf_validator;
use cf_traits::{
	impl_mock_chainflip,
	mocks::{
		qualify_node::QualifyAll, reputation_resetter::MockReputationResetter,
		vault_rotator::MockVaultRotatorA,
	},
	AccountRoleRegistry, Bid, RuntimeAuctionOutcome,
};
use frame_support::{
	construct_runtime, parameter_types,
	traits::{OnInitialize, ValidatorRegistration},
};
use sp_core::H256;
use sp_runtime::{
	impl_opaque_keys,
	testing::{Header, UintAuthorityId},
	traits::{BlakeTwo256, ConvertInto, IdentityLookup},
	BuildStorage,
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
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Index = u64;
	type BlockNumber = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = ValidatorId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = Header;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = BlockHashCount;
	type DbWeight = ();
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<5>;
}

impl_mock_chainflip!(Test);

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

impl pallet_session::Config for Test {
	type ShouldEndSession = ValidatorPallet;
	type SessionManager = ValidatorPallet;
	type SessionHandler = pallet_session::TestSessionHandler;
	type ValidatorId = ValidatorId;
	type ValidatorIdOf = ConvertInto;
	type Keys = MockSessionKeys;
	type RuntimeEvent = RuntimeEvent;
	type NextSessionRotation = ();
	type WeightInfo = ();
}
pub const AUCTION_WINNERS: [ValidatorId; 4] = [0, 1, 2, 3];
pub const WINNING_BIDS: [Amount; 4] = [120, 120, 110, 105];
pub const AUCTION_LOSERS: [ValidatorId; 3] = [5, 6, 7];
pub const UNQUALIFIED_NODE: ValidatorId = 8;
pub const UNQUALIFIED_NODE_BID: Amount = 200;
pub const LOSING_BIDS: [Amount; 3] = [99, 90, 74];
pub const EXPECTED_BOND: Amount = 105;

thread_local! {
	pub static NEXT_AUCTION_OUTCOME: RefCell<Result<RuntimeAuctionOutcome<Test>, &'static str>> = RefCell::new(Ok(
		RuntimeAuctionOutcome::<Test> {
			winners: AUCTION_WINNERS.to_vec(),
			losers: AUCTION_LOSERS.zip(LOSING_BIDS).map(Into::into).to_vec(),
			bond: *WINNING_BIDS.iter().min().unwrap(),
		}
	));

	pub static NUMBER_OF_AUCTIONS_ATTEMPTED: RefCell<u8> = RefCell::new(0);
}

impl ValidatorRegistration<ValidatorId> for Test {
	fn is_registered(_id: &ValidatorId) -> bool {
		true
	}
}

pub struct TestEpochTransitionHandler;

impl EpochTransitionHandler for TestEpochTransitionHandler {}

thread_local! {
	pub static MISSED_SLOTS: RefCell<(u64, u64)> = RefCell::new(Default::default());
	pub static BIDDERS: RefCell<Vec<Bid<ValidatorId, Amount>>> = RefCell::new(Default::default());
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

impl MockBidderProvider {
	pub fn set_bids(bids: Vec<Bid<ValidatorId, Amount>>) {
		BIDDERS.with(|cell| *cell.borrow_mut() = bids);
	}

	pub fn set_winning_bids() {
		BIDDERS.with(|cell| {
			*cell.borrow_mut() = AUCTION_WINNERS
				.zip(WINNING_BIDS)
				.into_iter()
				.chain(AUCTION_LOSERS.zip(LOSING_BIDS))
				.chain(sp_std::iter::once((UNQUALIFIED_NODE, UNQUALIFIED_NODE_BID)))
				.map(|(bidder_id, amount)| Bid { bidder_id, amount })
				.collect()
		})
	}
}

impl BidderProvider for MockBidderProvider {
	type ValidatorId = ValidatorId;
	type Amount = Amount;

	fn get_bidders() -> Vec<Bid<Self::ValidatorId, Self::Amount>> {
		BIDDERS.with(|cell| cell.borrow().clone())
	}
}

impl Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Offence = PalletOffence;
	type EpochTransitionHandler = TestEpochTransitionHandler;
	type MinEpoch = MinEpoch;
	type ValidatorWeightInfo = ();
	type VaultRotator = MockVaultRotatorA;
	type MissedAuthorshipSlots = MockMissedAuthorshipSlots;
	type BidderProvider = MockBidderProvider;
	type OffenceReporter = MockOffenceReporter;
	type Bonder = MockBonder;
	type ReputationResetter = MockReputationResetter<Self>;
	type KeygenQualification = QualifyAll<ValidatorId>;
}

/// Session pallet requires a set of validators at genesis.
pub const GENESIS_AUTHORITIES: [u64; 3] = [u64::MAX, u64::MAX - 1, u64::MAX - 2];
pub const REDEMPTION_PERCENTAGE_AT_GENESIS: Percentage = 50;
pub const GENESIS_BOND: Amount = 1;
pub const EPOCH_DURATION: u64 = 10;
pub(crate) struct TestExternalitiesWithCheck {
	ext: sp_io::TestExternalities,
}

#[macro_export]
macro_rules! assert_invariants {
	() => {
		assert_eq!(
			<ValidatorPallet as EpochInfo>::current_authorities()
				.into_iter()
				.collect::<Vec<_>>(),
			Session::validators(),
			"Authorities out of sync at block {:?}. RotationPhase: {:?}",
			System::block_number(),
			ValidatorPallet::current_rotation_phase(),
		);

		assert!(
			ValidatorPallet::current_authorities()
				.into_iter()
				.collect::<BTreeSet<_>>()
				.is_disjoint(&ValidatorPallet::highest_funded_qualified_backup_nodes_lookup()),
			"Backup nodes and validators should not overlap",
		);
	};
}

impl TestExternalitiesWithCheck {
	pub fn execute_with<R>(&mut self, execute: impl FnOnce() -> R) -> R {
		self.ext.execute_with(|| {
			System::set_block_number(1);
			QualifyAll::<u64>::except([UNQUALIFIED_NODE]);
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
			execute()
		})
	}
}

pub const MIN_AUTHORITY_SIZE: u32 = 1;
pub const MAX_AUTHORITY_SIZE: u32 = 5;
pub const MAX_AUTHORITY_SET_EXPANSION: u32 = 5;

pub(crate) fn new_test_ext() -> TestExternalitiesWithCheck {
	log::debug!("Initializing TestExternalitiesWithCheck with GenesisConfig.");

	TestExternalitiesWithCheck {
		ext: {
			let mut ext: sp_io::TestExternalities = GenesisConfig {
				system: SystemConfig::default(),
				session: SessionConfig {
					keys: [&GENESIS_AUTHORITIES[..], &AUCTION_WINNERS[..], &AUCTION_LOSERS[..]]
						.concat()
						.iter()
						.map(|&i| (i, i, UintAuthorityId(i).into()))
						.collect(),
				},
				validator_pallet: ValidatorPalletConfig {
					genesis_authorities: BTreeSet::from(GENESIS_AUTHORITIES),
					genesis_backups: Default::default(),
					genesis_vanity_names: BTreeMap::from_iter([(
						GENESIS_AUTHORITIES[0],
						"Alice ✅".as_bytes().to_vec(),
					)]),
					blocks_per_epoch: EPOCH_DURATION,
					bond: GENESIS_BOND,
					redemption_period_as_percentage: REDEMPTION_PERCENTAGE_AT_GENESIS,
					backup_reward_node_percentage: 34,
					authority_set_min_size: MIN_AUTHORITY_SIZE,
					min_size: MIN_AUTHORITY_SIZE,
					max_size: MAX_AUTHORITY_SIZE,
					max_expansion: MAX_AUTHORITY_SET_EXPANSION,
				},
			}
			.build_storage()
			.unwrap()
			.into();

			ext.execute_with(|| {
				assert_eq!(
					VanityNames::<Test>::get().get(&GENESIS_AUTHORITIES[0]).unwrap(),
					&"Alice ✅".as_bytes().to_vec()
				);
				for account_id in
					[&GENESIS_AUTHORITIES[..], &AUCTION_WINNERS[..], &AUCTION_LOSERS[..]]
						.into_iter()
						.flatten()
				{
					<<Test as Chainflip>::AccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_validator(account_id).unwrap();
				}
			});

			ext
		},
	}
}

pub fn run_to_block(n: u64) {
	assert_invariants!();
	while System::block_number() < n {
		log::debug!("Test::on_initialise({:?})", System::block_number());
		System::set_block_number(System::block_number() + 1);
		AllPalletsWithoutSystem::on_initialize(System::block_number());
		assert_invariants!();
	}
}

pub fn move_forward_blocks(n: u64) {
	run_to_block(System::block_number() + n);
}
