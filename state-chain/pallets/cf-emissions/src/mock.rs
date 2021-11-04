use std::marker::PhantomData;

use crate as pallet_cf_emissions;
use cf_chains::{eth, Ethereum};
use frame_support::{
	parameter_types,
	traits::{Imbalance, UnfilteredDispatchable},
};
use frame_system as system;
use pallet_cf_flip;
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

use cf_traits::{
	mocks::{ensure_origin_mock::NeverFailingOriginCheck, epoch_info},
	Chainflip, NonceProvider, RewardsDistribution, SigningContext, ThresholdSigner,
};

pub type AccountId = u64;

cf_traits::impl_mock_stake_transfer!(AccountId, u128);

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Module, Call, Config, Storage, Event<T>},
		Flip: pallet_cf_flip::{Module, Call, Config<T>, Storage, Event<T>},
		Emissions: pallet_cf_emissions::{Module, Call, Storage, Event<T>, Config},
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
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

cf_traits::impl_mock_ensure_witnessed_for_origin!(Origin);
cf_traits::impl_mock_offline_conditions!(u64);

impl Chainflip for Test {
	type KeyId = Vec<u8>;
	type ValidatorId = AccountId;
	type Amount = u128;
	type Call = Call;
	type EnsureWitnessed = MockEnsureWitnessed;
}

pub struct MockCallback;

impl UnfilteredDispatchable for MockCallback {
	type Origin = Origin;

	fn dispatch_bypass_filter(
		self,
		_origin: Self::Origin,
	) -> frame_support::dispatch::DispatchResultWithPostInfo {
		Ok(().into())
	}
}

pub struct MockEthSigningContext;

impl From<eth::update_flip_supply::UpdateFlipSupply> for MockEthSigningContext {
	fn from(_: eth::update_flip_supply::UpdateFlipSupply) -> Self {
		MockEthSigningContext
	}
}

impl SigningContext<Test> for MockEthSigningContext {
	type Chain = Ethereum;
	type Payload = Vec<u8>;
	type Signature = Vec<u8>;
	type Callback = MockCallback;

	fn get_payload(&self) -> Self::Payload {
		b"payloooooad".to_vec()
	}

	fn resolve_callback(&self, _signature: Self::Signature) -> Self::Callback {
		MockCallback
	}
}

pub struct MockThresholdSigner;

impl ThresholdSigner<Test> for MockThresholdSigner {
	type Context = MockEthSigningContext;

	fn request_signature(_context: Self::Context) -> u64 {
		0
	}
}

parameter_types! {
	pub const ExistentialDeposit: u128 = 10;
}

parameter_types! {
	pub const BlocksPerDay: u64 = 14400;
}

impl pallet_cf_flip::Config for Test {
	type Event = Event;
	type Balance = u128;
	type ExistentialDeposit = ExistentialDeposit;
	type EnsureGovernance = NeverFailingOriginCheck<Self>;
	type BlocksPerDay = BlocksPerDay;
	type StakeHandler = MockStakeHandler;
	type WeightInfo = ();
}

pub const NONCE: u64 = 42;

impl NonceProvider<Ethereum> for Test {
	fn next_nonce() -> cf_traits::Nonce {
		NONCE
	}
}

pub const MINT_INTERVAL: u64 = 5;

parameter_types! {
	pub const MintInterval: u64 = MINT_INTERVAL;

}

cf_traits::impl_mock_witnesser_for_account_and_call_types!(u64, Call);

pub struct MockRewardsDistribution<T>(PhantomData<T>);

impl RewardsDistribution for MockRewardsDistribution<Test> {
	type Balance = u128;
	type Surplus = pallet_cf_flip::Surplus<Test>;

	fn distribute(rewards: Self::Surplus) {
		let reward_amount = rewards.peek();
		let deposit = Flip::deposit_reserves(*b"RSVR", reward_amount);
		let _ = rewards.offset(deposit);
	}

	fn execution_weight() -> frame_support::dispatch::Weight {
		1
	}
}

impl pallet_cf_emissions::Config for Test {
	type Event = Event;
	type FlipBalance = u128;
	type Surplus = pallet_cf_flip::Surplus<Test>;
	type Issuance = pallet_cf_flip::FlipIssuance<Test>;
	type RewardsDistribution = MockRewardsDistribution<Self>;
	type MintInterval = MintInterval;
	type BlocksPerDay = BlocksPerDay;
	type NonceProvider = Self;
	type SigningContext = MockEthSigningContext;
	type ThresholdSigner = MockThresholdSigner;
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext(validators: Vec<u64>, issuance: Option<u128>) -> sp_io::TestExternalities {
	let total_issuance = issuance.unwrap_or(1_000_000_000u128);
	let config = GenesisConfig {
		frame_system: Default::default(),
		pallet_cf_flip: Some(FlipConfig { total_issuance }),
		pallet_cf_emissions: Some({
			EmissionsConfig {
				validator_emission_inflation: 1000,       // 10%
				backup_validator_emission_inflation: 100, // 1%
			}
		}),
	};

	for v in validators {
		epoch_info::Mock::add_validator(v);
	}

	config.build_storage().unwrap().into()
}
