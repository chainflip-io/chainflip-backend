use crate as pallet_cf_emissions;
use cf_chains::{eth, ChainCrypto, Ethereum};
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

use cf_traits::WaivedFees;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

use cf_traits::{
	impl_mock_waived_fees,
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
		System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
		Flip: pallet_cf_flip::{Pallet, Call, Config<T>, Storage, Event<T>},
		Emissions: pallet_cf_emissions::{Pallet, Call, Storage, Event<T>, Config},
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
}

impl system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
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
	type OnSetCode = ();
}

cf_traits::impl_mock_offline_conditions!(u64);

impl Chainflip for Test {
	type KeyId = Vec<u8>;
	type ValidatorId = AccountId;
	type Amount = u128;
	type Call = Call;
	type EnsureWitnessed = NeverFailingOriginCheck<Self>;
	type EpochInfo = cf_traits::mocks::epoch_info::MockEpochInfo;
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
	type Callback = MockCallback;
	type ThresholdSignatureOrigin = Origin;

	fn get_payload(&self) -> <Self::Chain as ChainCrypto>::Payload {
		Default::default()
	}

	fn resolve_callback(
		&self,
		_signature: <Self::Chain as ChainCrypto>::ThresholdSignature,
	) -> Self::Callback {
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

// Implement mock for RestrictionHandler
impl_mock_waived_fees!(AccountId, Call);

impl pallet_cf_flip::Config for Test {
	type Event = Event;
	type Balance = u128;
	type ExistentialDeposit = ExistentialDeposit;
	type EnsureGovernance = NeverFailingOriginCheck<Self>;
	type BlocksPerDay = BlocksPerDay;
	type StakeHandler = MockStakeHandler;
	type WeightInfo = ();
	type WaivedFees = WaivedFeesMock;
}

pub const NONCE: u64 = 42;

impl NonceProvider<Ethereum> for Test {
	fn next_nonce() -> cf_traits::Nonce {
		NONCE
	}
}

pub const MINT_INTERVAL: u64 = 100;

parameter_types! {
	pub const MintInterval: u64 = MINT_INTERVAL;
}

cf_traits::impl_mock_witnesser_for_account_and_call_types!(u64, Call, u64);

pub struct MockRewardsDistribution;

impl RewardsDistribution for MockRewardsDistribution {
	type Balance = u128;
	type Surplus = pallet_cf_flip::Surplus<Test>;

	fn distribute(rewards: Self::Surplus) {
		let reward_amount = rewards.peek();
		let deposit = Flip::deposit_reserves(*b"RSVR", reward_amount);
		let _ = rewards.offset(deposit);
	}
}

impl pallet_cf_emissions::Config for Test {
	type Event = Event;
	type FlipBalance = u128;
	type Surplus = pallet_cf_flip::Surplus<Test>;
	type Issuance = pallet_cf_flip::FlipIssuance<Test>;
	type RewardsDistribution = MockRewardsDistribution;
	type BlocksPerDay = BlocksPerDay;
	type NonceProvider = Self;
	type SigningContext = MockEthSigningContext;
	type ThresholdSigner = MockThresholdSigner;
	type WeightInfo = ();
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext(validators: Vec<u64>, issuance: Option<u128>) -> sp_io::TestExternalities {
	let total_issuance = issuance.unwrap_or(1_000_000_000u128);
	let config = GenesisConfig {
		system: Default::default(),
		flip: FlipConfig { total_issuance },
		emissions: {
			EmissionsConfig {
				validator_emission_inflation: 1000,       // 10%
				backup_validator_emission_inflation: 100, // 1%
			}
		},
	};

	for v in validators {
		epoch_info::Mock::add_validator(v);
	}

	config.build_storage().unwrap().into()
}
