use std::cell::RefCell;

use crate::{self as pallet_cf_tokenholder_governance};
use cf_chains::{mocks::MockEthereum, ApiCall, eth::api::EthereumReplayProtection, ChainAbi, SetGovKey, ChainCrypto};
use cf_traits::{
	mocks::{epoch_info::MockEpochInfo, system_state_info::MockSystemStateInfo, time_source},
	Chainflip, ExecutionCondition, RuntimeUpgrade, FeePayment, StakingInfo, Broadcaster, ReplayProtectionProvider,
};
use codec::{MaxEncodedLen, Encode, Decode};
use frame_support::{dispatch::DispatchResultWithPostInfo, ensure, parameter_types, storage, Twox64Concat, StorageHasher};
use frame_system as system;
use scale_info::TypeInfo;
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};

use cf_chains::SetCommunityKey;
use cf_traits::AuthorityKeys;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;
type AccountId = u64;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system,
		TokenholderGovernance: pallet_cf_tokenholder_governance,
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
}

pub const FAKE_KEYMAN_ADDR: [u8; 20] = [0xcf; 20];
pub const CHAIN_ID: u64 = 31337;
pub const COUNTER: u64 = 42;

pub struct MockReplayProvider;

impl ReplayProtectionProvider<MockEthereum> for MockReplayProvider {
	fn replay_protection() -> <MockEthereum as ChainAbi>::ReplayProtection {
		EthereumReplayProtection {
			key_manager_address: FAKE_KEYMAN_ADDR,
			chain_id: CHAIN_ID,
			nonce: COUNTER,
		}
	}
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
	type AccountId = AccountId;
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
	type MaxConsumers = frame_support::traits::ConstU32<5>;
}

cf_traits::impl_mock_ensure_witnessed_for_origin!(Origin);

pub struct MockFeePayment;
pub struct MockStakingInfo;

impl StakingInfo for MockStakingInfo {
    type AccountId = AccountId;

    type Balance = u128;

    fn total_balance_of(account_id: &Self::AccountId) -> Self::Balance {
        todo!()
    }

    fn onchain_funds() -> Self::Balance {
        todo!()
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct MockSetGovKey {
	pub nonce: <MockEthereum as ChainAbi>::ReplayProtection,
	pub new_key: cf_chains::eth::Address,
}

impl SetGovKey<MockEthereum> for MockSetGovKey {
	fn new_unsigned(
		nonce: <MockEthereum as ChainAbi>::ReplayProtection,
		new_key: cf_chains::eth::Address,
	) -> Self {
		Self {
			nonce,
			new_key
		}
	}
}

impl ApiCall<MockEthereum> for MockSetGovKey {
	fn threshold_signature_payload(&self) -> <MockEthereum as ChainCrypto>::Payload {
		[0xcf; 4]
	}

	fn signed(
		self,
		_threshold_signature: &<MockEthereum as ChainCrypto>::ThresholdSignature,
	) -> Self {
		unimplemented!()
	}

	fn abi_encoded(&self) -> Vec<u8> {
		unimplemented!()
	}

	fn is_signed(&self) -> bool {
		unimplemented!()
	}
}


#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct MockBroadcastGov;

impl MockBroadcastGov {
	pub fn call(outgoing: MockSetGovKey) {
		storage::hashed::put(&<Twox64Concat as StorageHasher>::hash, b"MockBroadcastGov", &outgoing)
	}

	pub fn get_called() -> Option<MockSetGovKey> {
		storage::hashed::get(&<Twox64Concat as StorageHasher>::hash, b"MockBroadcastGov")
	}
}

impl Broadcaster<MockEthereum> for MockBroadcastGov {
	type ApiCall = MockSetGovKey;

	fn threshold_sign_and_broadcast(api_call: Self::ApiCall) {
		Self::call(api_call)
	}
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct MockSetCommKey {
	pub nonce: <MockEthereum as ChainAbi>::ReplayProtection,
	pub new_key: cf_chains::eth::Address,
}

impl SetCommunityKey<MockEthereum> for MockSetCommKey {
	fn new_unsigned(
		nonce: <MockEthereum as ChainAbi>::ReplayProtection,
		new_key: cf_chains::eth::Address,
	) -> Self {
		Self {
			nonce,
			new_key
		}
	}
}

impl ApiCall<MockEthereum> for MockSetCommKey {
	fn threshold_signature_payload(&self) -> <MockEthereum as ChainCrypto>::Payload {
		[0xcf; 4]
	}

	fn signed(
		self,
		_threshold_signature: &<MockEthereum as ChainCrypto>::ThresholdSignature,
	) -> Self {
		unimplemented!()
	}

	fn abi_encoded(&self) -> Vec<u8> {
		unimplemented!()
	}

	fn is_signed(&self) -> bool {
		unimplemented!()
	}
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct MockBroadcastComm;

impl MockBroadcastComm {
	pub fn call(outgoing: MockSetCommKey) {
		storage::hashed::put(&<Twox64Concat as StorageHasher>::hash, b"MockBroadcastGov", &outgoing)
	}

	pub fn get_called() -> Option<MockSetCommKey> {
		storage::hashed::get(&<Twox64Concat as StorageHasher>::hash, b"MockBroadcastGov")
	}
}

impl Broadcaster<MockEthereum> for MockBroadcastComm {
	type ApiCall = MockSetCommKey;

	fn threshold_sign_and_broadcast(api_call: Self::ApiCall) {
		Self::call(api_call)
	}
}

impl FeePayment for MockFeePayment {
    type AccountId = AccountId;
    type Amount = u128;
    fn try_burn_fee(account_id: Self::AccountId, amount: Self::Amount) -> Result<(), ()> {
        Ok(().into())
    }
}

pub struct MockKeys;

impl AuthorityKeys for MockKeys {
	type Gov = [u8; 32];
	type Comm = [u8; 32];
}

impl Chainflip for Test {
	type KeyId = Vec<u8>;
	type ValidatorId = u64;
	type Amount = u128;
	type Call = Call;
	type EnsureWitnessed = MockEnsureWitnessed;
	type EnsureWitnessedAtCurrentEpoch = MockEnsureWitnessed;
	type EpochInfo = MockEpochInfo;
	type SystemState = MockSystemStateInfo;
}

impl pallet_cf_tokenholder_governance::Config for Test {
    type Event = Event;
    type Balance = u128;
    type FeePayment = MockFeePayment;
	type Chain = MockEthereum;
    type ReplayProtectionProvider = MockReplayProvider;
    type Flip = MockStakingInfo;
	type SetGovKeyApiCall = MockSetGovKey;
	type GovKeyBroadcaster = MockBroadcastGov;
	type SetCommunityKeyApiCall = MockSetCommKey;
	type CommKeyBroadcaster = MockBroadcastComm;
	type Keys = MockKeys;
}

pub const ALICE: <Test as frame_system::Config>::AccountId = 123u64;
pub const BOB: <Test as frame_system::Config>::AccountId = 456u64;
pub const CHARLES: <Test as frame_system::Config>::AccountId = 789u64;
pub const EVE: <Test as frame_system::Config>::AccountId = 987u64;
pub const PETER: <Test as frame_system::Config>::AccountId = 988u64;
pub const MAX: <Test as frame_system::Config>::AccountId = 989u64;

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let config = GenesisConfig {
		system: Default::default(),
	};

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		// This is required to log events.
		System::set_block_number(1);
	});

	ext
}
