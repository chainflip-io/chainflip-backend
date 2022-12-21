use crate::{self as pallet_cf_tokenholder_governance};
use cf_chains::{mocks::MockEthereum, ApiCall, ChainAbi, ChainCrypto};
use cf_primitives::BroadcastId;
use cf_traits::{
	impl_mock_stake_transfer, impl_mock_waived_fees,
	mocks::{
		ensure_origin_mock::NeverFailingOriginCheck, epoch_info::MockEpochInfo,
		system_state_info::MockSystemStateInfo,
	},
	Broadcaster, Chainflip, StakeTransfer, WaivedFees,
};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	parameter_types, storage, traits::HandleLifetime, StorageHasher, Twox64Concat,
};
use frame_system as system;
use scale_info::TypeInfo;
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};

use cf_chains::{SetCommKeyWithAggKey, SetGovKeyWithAggKey};
use system::pallet_prelude::BlockNumberFor;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;
type AccountId = u64;
type Balance = u128;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system,
		TokenholderGovernance: pallet_cf_tokenholder_governance,
		Flip: pallet_cf_flip,
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
	pub const VotingPeriod: BlockNumberFor<Test> = 10;
	pub const ProposalFee: Balance = 100;
	pub const EnactmentDelay: BlockNumberFor<Test> = 20;
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

cf_traits::impl_mock_ensure_witnessed_for_origin!(Origin);

#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct MockApiCalls {
	pub nonce: <MockEthereum as ChainAbi>::ReplayProtection,
	pub new_key: <MockEthereum as ChainCrypto>::GovKey,
}

impl SetGovKeyWithAggKey<MockEthereum> for MockApiCalls {
	fn new_unsigned(new_key: <MockEthereum as ChainCrypto>::GovKey) -> Self {
		Self { nonce: Default::default(), new_key }
	}
}

impl ApiCall<MockEthereum> for MockApiCalls {
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
pub struct MockBroadcaster;

impl SetCommKeyWithAggKey<MockEthereum> for MockApiCalls {
	fn new_unsigned(new_key: <MockEthereum as ChainCrypto>::GovKey) -> Self {
		Self { nonce: Default::default(), new_key }
	}
}

impl Broadcaster<MockEthereum> for MockBroadcaster {
	type ApiCall = MockApiCalls;

	fn threshold_sign_and_broadcast(api_call: Self::ApiCall) -> BroadcastId {
		storage::hashed::put(&<Twox64Concat as StorageHasher>::hash, b"GOV", &api_call);
		1
	}
}

impl MockBroadcaster {
	pub fn get_called() -> Option<<MockBroadcaster as Broadcaster<MockEthereum>>::ApiCall> {
		storage::hashed::get(&<Twox64Concat as StorageHasher>::hash, b"GOV")
	}
}

impl Chainflip for Test {
	type KeyId = Vec<u8>;
	type ValidatorId = u64;
	type Amount = u128;
	type RuntimeCall = RuntimeCall;
	type EnsureWitnessed = MockEnsureWitnessed;
	type EnsureWitnessedAtCurrentEpoch = MockEnsureWitnessed;
	type EpochInfo = MockEpochInfo;
	type SystemState = MockSystemStateInfo;
}

parameter_types! {
	pub const ExistentialDeposit: u128 = 10;
}

parameter_types! {
	pub const BlocksPerDay: u64 = 14400;
}

// Implement mock for RestrictionHandler
impl_mock_waived_fees!(AccountId, Call);
impl_mock_stake_transfer!(AccountId, u128);

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

impl pallet_cf_tokenholder_governance::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type FeePayment = Flip;
	type Chain = MockEthereum;
	type StakingInfo = Flip;
	type ApiCalls = MockApiCalls;
	type Broadcaster = MockBroadcaster;
	type WeightInfo = ();
	type VotingPeriod = VotingPeriod;
	type EnactmentDelay = EnactmentDelay;
	type ProposalFee = ProposalFee;
}

// Accounts
pub const ALICE: AccountId = 123u64;
pub const BOB: AccountId = 456u64;
pub const CHARLES: AccountId = 789u64;
pub const EVE: AccountId = 987u64;
pub const BROKE_PAUL: AccountId = 1987u64;

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let stakes = [
		(ALICE, 500),
		(BOB, 200),
		(CHARLES, 100),
		(EVE, 200),
		(BROKE_PAUL, ProposalFee::get() - 1),
	];
	let total_issuance = stakes.iter().map(|(_, stake)| stake).sum();
	let config = GenesisConfig { system: Default::default(), flip: FlipConfig { total_issuance } };

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
		for (account, stake) in stakes {
			frame_system::Provider::<Test>::created(&account).unwrap();
			assert!(frame_system::Pallet::<Test>::account_exists(&account));
			<Flip as StakeTransfer>::credit_stake(&account, stake);
		}
	});

	ext
}
