use crate as pallet_cf_staking;
use cf_chains::{ApiCall, Chain, ChainCrypto, Ethereum};
use cf_primitives::AuthorityCount;
use cf_traits::{
	impl_mock_waived_fees,
	mocks::{
		bid_info::MockBidInfo, staking_info::MockStakingInfo,
		system_state_info::MockSystemStateInfo,
	},
	Broadcaster, WaivedFees,
};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::parameter_types;
use scale_info::TypeInfo;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
	AccountId32, BuildStorage,
};
use std::time::Duration;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;
// Use a realistic account id for compatibility with `RegisterClaim`.
type AccountId = AccountId32;
type Balance = u128;

use cf_traits::{
	mocks::{ensure_origin_mock::NeverFailingOriginCheck, time_source},
	Chainflip,
};

impl pallet_cf_account_roles::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type BidInfo = MockBidInfo;
	type EnsureGovernance = NeverFailingOriginCheck<Self>;
	type StakeInfo = MockStakingInfo<Self>;
	type WeightInfo = ();
}

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system,
		AccountRoles: pallet_cf_account_roles,
		Flip: pallet_cf_flip,
		Staking: pallet_cf_staking,
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
}

impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Index = u64;
	type BlockNumber = u64;
	type Hash = sp_core::H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
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
	type Amount = Balance;
	type RuntimeCall = RuntimeCall;
	type EnsureWitnessed = MockEnsureWitnessed;
	type EnsureWitnessedAtCurrentEpoch = MockEnsureWitnessed;
	type EpochInfo = MockEpochInfo;
	type SystemState = MockSystemStateInfo;
}

parameter_types! {
	pub const CeremonyRetryDelay: <Test as frame_system::Config>::BlockNumber = 1;
}

parameter_types! {
	pub const ExistentialDeposit: Balance = 10;
}

parameter_types! {
	pub const BlocksPerDay: u64 = 14400;
}

// Implement mock for RestrictionHandler
impl_mock_waived_fees!(AccountId, RuntimeCall);

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

cf_traits::impl_mock_ensure_witnessed_for_origin!(RuntimeOrigin);
cf_traits::impl_mock_epoch_info!(AccountId, u128, u32, AuthorityCount);
cf_traits::impl_mock_stake_transfer!(AccountId, u128);

pub struct MockBroadcaster;

thread_local! {
	pub static CLAIM_BROADCAST_REQUESTS: RefCell<Vec<<Ethereum as Chain>::ChainAmount>> = RefCell::new(vec![]);
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct MockRegisterClaim {
	amount: <Ethereum as Chain>::ChainAmount,
}

impl cf_chains::RegisterClaim<Ethereum> for MockRegisterClaim {
	fn new_unsigned(_node_id: &[u8; 32], amount: u128, _address: &[u8; 20], _expiry: u64) -> Self {
		Self { amount }
	}

	fn amount(&self) -> u128 {
		self.amount
	}
}

impl ApiCall<Ethereum> for MockRegisterClaim {
	fn threshold_signature_payload(&self) -> <Ethereum as ChainCrypto>::Payload {
		unimplemented!()
	}

	fn signed(self, _threshold_signature: &<Ethereum as ChainCrypto>::ThresholdSignature) -> Self {
		unimplemented!()
	}

	fn chain_encoded(&self) -> Vec<u8> {
		unimplemented!()
	}

	fn is_signed(&self) -> bool {
		unimplemented!()
	}
}

impl MockBroadcaster {
	pub fn received_requests() -> Vec<<Ethereum as Chain>::ChainAmount> {
		CLAIM_BROADCAST_REQUESTS.with(|cell| cell.borrow().clone())
	}
}

impl Broadcaster<Ethereum> for MockBroadcaster {
	type ApiCall = MockRegisterClaim;

	fn threshold_sign_and_broadcast(api_call: Self::ApiCall) -> cf_primitives::BroadcastId {
		CLAIM_BROADCAST_REQUESTS.with(|cell| {
			cell.borrow_mut().push(api_call.amount);
		});
		0
	}
}

pub const CLAIM_DELAY_BUFFER_SECS: u64 = 10;

impl pallet_cf_staking::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type TimeSource = time_source::Mock;
	type Balance = u128;
	type AccountRoleRegistry = ();
	type Flip = Flip;
	type WeightInfo = ();
	type StakerId = AccountId;
	type Broadcaster = MockBroadcaster;
	type ThresholdCallable = RuntimeCall;
	type EnsureThresholdSigned = NeverFailingOriginCheck<Self>;
	type EnsureGovernance = NeverFailingOriginCheck<Self>;
	type RegisterClaim = MockRegisterClaim;
}

pub const CLAIM_TTL_SECS: u64 = 10;

pub const ALICE: AccountId = AccountId32::new([0xa1; 32]);
pub const BOB: AccountId = AccountId32::new([0xb0; 32]);
// Used as genesis node for testing.
pub const CHARLIE: AccountId = AccountId32::new([0xc1; 32]);

pub const MIN_STAKE: u128 = 10;
// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let config = GenesisConfig {
		account_roles: Default::default(),
		system: Default::default(),
		flip: FlipConfig { total_issuance: 1_000_000 },
		staking: StakingConfig {
			genesis_stakers: vec![(CHARLIE, MIN_STAKE)],
			minimum_stake: MIN_STAKE,
			claim_ttl: Duration::from_secs(CLAIM_TTL_SECS),
			claim_delay_buffer_seconds: CLAIM_DELAY_BUFFER_SECS,
		},
	};

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
