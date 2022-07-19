use crate::{self as pallet_cf_tokenholder_governance};
use cf_chains::{
	eth::api::{EthereumReplayProtection}, mocks::MockEthereum, ApiCall, ChainAbi, ChainCrypto,
};
use cf_traits::{
	mocks::{epoch_info::MockEpochInfo, system_state_info::MockSystemStateInfo},
	Broadcaster, Chainflip, FeePayment, ReplayProtectionProvider, StakingInfo,
};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{parameter_types, storage, StorageHasher, Twox64Concat};
use frame_system as system;
use scale_info::TypeInfo;
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage, DispatchError,
};

use cf_chains::SetGovKeyWithAggKey;
use cf_chains::SetCommKeyWithAggKey;

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

	fn total_stake_of(account_id: &Self::AccountId) -> Self::Balance {
		match account_id {
			&ALICE => ALICE_BALANCE,
			&BOB => BOB_BALANCE,
			&CHARLES => CHARLES_BALANCE,
			&EVE => EVE_BALANCE,
			_ => 0,
		}
	}

	fn total_onchain_stake() -> Self::Balance {
		10000
	}
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct MockApiCalls {
	pub nonce: <MockEthereum as ChainAbi>::ReplayProtection,
	pub new_key: <MockEthereum as ChainCrypto>::GovKey,
}

impl SetGovKeyWithAggKey<MockEthereum> for MockApiCalls {
	fn new_unsigned(
		nonce: <MockEthereum as ChainAbi>::ReplayProtection,
		new_key: <MockEthereum as ChainCrypto>::GovKey,
	) -> Self {
		Self { nonce, new_key }
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

	fn abi_encoded(&self) -> Vec<u8> {
		unimplemented!()
	}

	fn is_signed(&self) -> bool {
		unimplemented!()
	}
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct MockBroadcaster;

impl SetCommKeyWithAggKey<MockEthereum> for MockApiCalls {
	fn new_unsigned(
		nonce: <MockEthereum as ChainAbi>::ReplayProtection,
		new_key: <MockEthereum as ChainCrypto>::GovKey,
	) -> Self {
		Self { nonce, new_key }
	}
}

impl Broadcaster<MockEthereum> for MockBroadcaster {
	type ApiCall = MockApiCalls;

	fn threshold_sign_and_broadcast(api_call: Self::ApiCall) {
		storage::hashed::put(&<Twox64Concat as StorageHasher>::hash, b"GOV", &api_call);
	}
}

impl MockBroadcaster {
	pub fn get_called() -> Option<<MockBroadcaster as Broadcaster<MockEthereum>>::ApiCall> {
		storage::hashed::get(&<Twox64Concat as StorageHasher>::hash, b"GOV")
	}
}

impl FeePayment for MockFeePayment {
	type AccountId = AccountId;
	type Amount = Balance;
	fn try_burn_fee(
		account_id: Self::AccountId,
		amount: Self::Amount,
	) -> Result<(), sp_runtime::DispatchError> {
		let not_enough_funds = DispatchError::Other("Account is not sufficiently funded!");
		match account_id {
			ALICE if amount > ALICE_BALANCE => Err(not_enough_funds),
			BOB if amount > BOB_BALANCE => Err(not_enough_funds),
			CHARLES if amount > CHARLES_BALANCE => Err(not_enough_funds),
			EVE if amount > EVE_BALANCE => Err(not_enough_funds),
			_ => Ok(().into()),
		}
	}
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
	type StakingInfo = MockStakingInfo;
	type ApiCalls = MockApiCalls;
	type Broadcaster = MockBroadcaster;
	type WeightInfo = ();
}

// Accounts
pub const ALICE: <Test as frame_system::Config>::AccountId = 123u64;
pub const BOB: <Test as frame_system::Config>::AccountId = 456u64;
pub const CHARLES: <Test as frame_system::Config>::AccountId = 789u64;
pub const EVE: <Test as frame_system::Config>::AccountId = 987u64;

// Balances
pub const ALICE_BALANCE: Balance = 3000;
pub const BOB_BALANCE: Balance = 2000;
pub const CHARLES_BALANCE: Balance = 3000;
pub const EVE_BALANCE: Balance = 2000;

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let config = GenesisConfig { system: Default::default() };

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
