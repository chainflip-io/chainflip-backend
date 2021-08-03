use std::time::Duration;

use crate as pallet_cf_witness_api;
use pallet_cf_vaults::chains::ethereum as pallet_cf_ethereum;

use cf_traits::{
	impl_mock_ensure_witnessed_for_origin, impl_mock_stake_transfer,
	impl_mock_witnesser_for_account_and_call_types, AuctionPenalty,
};
use frame_support::parameter_types;
use frame_system as system;
use pallet_cf_vaults::rotation::ChainFlip;
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
		Vaults: pallet_cf_vaults::{Module, Call, Event<T>, Config},
		EthereumChain: pallet_cf_ethereum::{Module, Call, Event<T>, Config},
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

type Amount = u64;
type ValidatorId = u64;

impl ChainFlip for Test {
	type Amount = Amount;
	type ValidatorId = ValidatorId;
}

impl AuctionPenalty<ValidatorId> for Test {
	fn abort() {}
	fn penalise(_bad_validators: Vec<ValidatorId>) {}
}

impl pallet_cf_vaults::chains::ethereum::Config for Test {
	type Event = Event;
	type Vaults = Vaults;
	type EnsureWitnessed = MockEnsureWitnessed;
	type Nonce = u64;
	type NonceProvider =
		pallet_cf_vaults::nonce::NonceUnixTime<u64, cf_traits::mocks::time_source::Mock>;
	type RequestIndex = u64;
	type PublicKey = Vec<u8>;
}

impl pallet_cf_vaults::Config for Test {
	type Event = Event;
	type EthereumVault = EthereumChain;
	type EnsureWitnessed = MockEnsureWitnessed;
	type RequestIndex = u64;
	type Bytes = Vec<u8>;
	type Penalty = Self;
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
