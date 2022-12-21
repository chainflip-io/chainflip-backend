use crate as pallet_cf_emissions;
use cf_chains::{
	mocks::MockEthereum, ApiCall, ChainAbi, ChainCrypto, ReplayProtectionProvider, UpdateFlipSupply,
};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	parameter_types, storage,
	traits::{Imbalance, UnfilteredDispatchable},
	StorageHasher, Twox64Concat,
};
use frame_system as system;
use scale_info::TypeInfo;
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};

use cf_traits::{
	mocks::{
		eth_environment_provider::MockEthEnvironmentProvider,
		eth_replay_protection_provider::MockEthReplayProtectionProvider,
		system_state_info::MockSystemStateInfo,
	},
	Broadcaster, Issuance, WaivedFees,
};

use cf_primitives::{BroadcastId, FlipBalance};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

use cf_traits::{
	impl_mock_waived_fees,
	mocks::{ensure_origin_mock::NeverFailingOriginCheck, epoch_info},
	Chainflip, RewardsDistribution,
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
		System: frame_system,
		Flip: pallet_cf_flip,
		Emissions: pallet_cf_emissions,
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
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Index = u64;
	type BlockNumber = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = Header;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = BlockHashCount;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = SS58Prefix;
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<5>;
}

impl Chainflip for Test {
	type KeyId = Vec<u8>;
	type ValidatorId = AccountId;
	type Amount = u128;
	type RuntimeCall = RuntimeCall;
	type EnsureWitnessed = NeverFailingOriginCheck<Self>;
	type EnsureWitnessedAtCurrentEpoch = NeverFailingOriginCheck<Self>;
	type EpochInfo = cf_traits::mocks::epoch_info::MockEpochInfo;
	type SystemState = MockSystemStateInfo;
}

pub struct MockCallback;

impl UnfilteredDispatchable for MockCallback {
	type RuntimeOrigin = RuntimeOrigin;

	fn dispatch_bypass_filter(
		self,
		_origin: Self::Origin,
	) -> frame_support::dispatch::DispatchResultWithPostInfo {
		Ok(().into())
	}
}

parameter_types! {
	pub const ExistentialDeposit: u128 = 10;
}

parameter_types! {
	pub const BlocksPerDay: u64 = 14400;
}

parameter_types! {
	pub const HeartbeatBlockInterval: u64 = 150;
}

// Implement mock for RestrictionHandler
impl_mock_waived_fees!(AccountId, Call);

impl pallet_cf_flip::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Balance = u128;
	type ExistentialDeposit = ExistentialDeposit;
	type EnsureGovernance = NeverFailingOriginCheck<Self>;
	type BlocksPerDay = BlocksPerDay;
	type StakeHandler = MockStakeHandler;
	type WeightInfo = ();
	type WaivedFees = WaivedFeesMock;
}

pub const EMISSION_RATE: u128 = 10;
pub struct MockRewardsDistribution;

impl RewardsDistribution for MockRewardsDistribution {
	type Balance = u128;
	type Issuance = pallet_cf_flip::FlipIssuance<Test>;

	fn distribute() {
		let deposit =
			Flip::deposit_reserves(*b"RSVR", Emissions::current_authority_emission_per_block());
		let amount = deposit.peek();
		let _result = deposit.offset(Self::Issuance::mint(amount));
	}
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct MockUpdateFlipSupply {
	pub nonce: <MockEthereum as ChainAbi>::ReplayProtection,
	pub new_total_supply: u128,
	pub block_number: u64,
	pub stake_manager_address: [u8; 20],
}

impl UpdateFlipSupply<MockEthereum> for MockUpdateFlipSupply {
	fn new_unsigned(
		new_total_supply: u128,
		block_number: u64,
		stake_manager_address: &[u8; 20],
	) -> Self {
		Self {
			nonce: MockEthReplayProtectionProvider::replay_protection(),
			new_total_supply,
			block_number,
			stake_manager_address: *stake_manager_address,
		}
	}
}

impl ApiCall<MockEthereum> for MockUpdateFlipSupply {
	fn threshold_signature_payload(&self) -> <MockEthereum as ChainCrypto>::Payload {
		[0xcf; 4]
	}

	fn signed(
		self,
		_threshold_signature: &<MockEthereum as ChainCrypto>::ThresholdSignature,
	) -> Self {
		unimplemented!()
	}

	fn chain_encoded(&self) -> Vec<u8> {
		unimplemented!()
	}

	fn is_signed(&self) -> bool {
		unimplemented!()
	}
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct MockBroadcast;

impl MockBroadcast {
	pub fn call(outgoing: MockUpdateFlipSupply) -> u32 {
		storage::hashed::put(&<Twox64Concat as StorageHasher>::hash, b"MockBroadcast", &outgoing);
		1
	}

	pub fn get_called() -> Option<MockUpdateFlipSupply> {
		storage::hashed::get(&<Twox64Concat as StorageHasher>::hash, b"MockBroadcast")
	}
}

impl Broadcaster<MockEthereum> for MockBroadcast {
	type ApiCall = MockUpdateFlipSupply;

	fn threshold_sign_and_broadcast(api_call: Self::ApiCall) -> BroadcastId {
		Self::call(api_call)
	}
}

impl pallet_cf_emissions::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type HostChain = MockEthereum;
	type FlipBalance = FlipBalance;
	type ApiCall = MockUpdateFlipSupply;
	type Surplus = pallet_cf_flip::Surplus<Test>;
	type Issuance = pallet_cf_flip::FlipIssuance<Test>;
	type RewardsDistribution = MockRewardsDistribution;
	type CompoundingInterval = HeartbeatBlockInterval;
	type EthEnvironmentProvider = MockEthEnvironmentProvider;
	type Broadcaster = MockBroadcast;
	type WeightInfo = ();
	type EnsureGovernance = NeverFailingOriginCheck<Self>;
}

pub const SUPPLY_UPDATE_INTERVAL: u32 = 10;

// Build genesis storage according to the mock runtime.
pub fn new_test_ext(validators: Vec<u64>, issuance: Option<u128>) -> sp_io::TestExternalities {
	let total_issuance = issuance.unwrap_or(1_000_000_000u128);
	let config = GenesisConfig {
		system: Default::default(),
		flip: FlipConfig { total_issuance },
		emissions: {
			EmissionsConfig {
				current_authority_emission_inflation: 2720,
				backup_node_emission_inflation: 284,
				supply_update_interval: SUPPLY_UPDATE_INTERVAL,
			}
		},
	};

	for v in validators {
		epoch_info::Mock::add_authorities(v);
	}

	config.build_storage().unwrap().into()
}
