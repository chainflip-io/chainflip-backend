use std::time::Duration;

use crate as pallet_cf_witness_api;
use cf_traits::{
	impl_mock_ensure_witnessed_for_origin, impl_mock_witnesser_for_account_and_call_types, impl_mock_stake_transfer
};
use frame_support::parameter_types;
use frame_system as system;
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Module, Call, Config, Storage, Event<T>},
		Staking: pallet_cf_staking::{Module, Call, Event<T>, Config<T>},
		WitnessApi: pallet_cf_witness_api::{Module, Call},
	}
);

impl_mock_witnesser_for_account_and_call_types!(u64, Call);
impl_mock_ensure_witnessed_for_origin!(Origin);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
	pub const MinClaimTTL: Duration = Duration::from_millis(100);
	pub const ClaimTTL: Duration = Duration::from_millis(1000);
}

impl system::Config for Test {
	type BaseCallFilter = ();
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
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
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = SS58Prefix;
}

// pub struct MockStakeTransfer;
// 
// impl MockStakeTransfer {
// 	pub fn get_balance(account_id: u64) -> u128 {
// 		BALANCES.with(|cell| {
// 			cell.borrow()
// 				.get(&account_id)
// 				.map(ToOwned::to_owned)
// 				.unwrap_or_default()
// 		})
// 	}
// }

// thread_local! {
// 	pub static BALANCES: RefCell<HashMap<u64, u128>> = RefCell::new(HashMap::default());
// }

// impl cf_traits::StakeTransfer for MockStakeTransfer {
// 	type AccountId = u64;
// 	type Balance = u128;

// 	fn stakeable_balance(account_id: &Self::AccountId) -> Self::Balance {
// 		Self::get_balance(*account_id)
// 	}
// 	fn claimable_balance(account_id: &Self::AccountId) -> Self::Balance {
// 		Self::get_balance(*account_id)
// 	}
// 	fn credit_stake(account_id: &Self::AccountId, amount: Self::Balance) -> Self::Balance {
// 		BALANCES.with(|cell| *cell.borrow_mut().entry(*account_id).or_default() += amount);
// 		Self::get_balance(*account_id)
// 	}
// 	fn try_claim(
// 		account_id: &Self::AccountId,
// 		amount: Self::Balance,
// 	) -> Result<(), sp_runtime::DispatchError> {
// 		BALANCES.with(|cell| {
// 			cell.borrow_mut()
// 				.entry(*account_id)
// 				.or_default()
// 				.checked_sub(amount)
// 				.map(|_| ())
// 				.ok_or("Overflow".into())
// 		})
// 	}
// 	fn settle_claim(_amount: Self::Balance) {
// 		unimplemented!()
// 	}
// 	fn revert_claim(account_id: &Self::AccountId, amount: Self::Balance) {
// 		Self::credit_stake(account_id, amount);
// 	}
// }

impl_mock_stake_transfer!(u64, u128);

impl pallet_cf_staking::Config for Test {
	type Event = Event;
	type Balance = u128;
	type Flip = MockStakeTransfer;
	type Nonce = u64;
	type EnsureWitnessed = MockEnsureWitnessed;
	type EpochInfo = cf_traits::mocks::epoch_info::Mock;
	type TimeSource = cf_traits::mocks::time_source::Mock;
	type MinClaimTTL = MinClaimTTL;
	type ClaimTTL = ClaimTTL;
}

impl pallet_cf_witness_api::Config for Test {
	type Call = Call;
	type Witnesser = MockWitnesser;
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	system::GenesisConfig::default()
		.build_storage::<Test>()
		.unwrap()
		.into()
}
