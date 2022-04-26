use crate as pallet_cf_witness_api;

use cf_chains::{eth::api::EthereumApi, mocks::MockTransactionBuilder, Ethereum};
use cf_traits::{
	impl_mock_stake_transfer, impl_mock_witnesser_for_account_and_call_types,
	mocks::{
		ceremony_id_provider::MockCeremonyIdProvider, ensure_origin_mock::NeverFailingOriginCheck,
		epoch_info::MockEpochInfo, eth_environment_provider::MockEthEnvironmentProvider,
		key_provider::MockKeyProvider, nonce_provider::MockEthReplayProtectionProvider,
	},
	Chainflip,
};
use codec::{Decode, Encode};
use frame_support::{instances::Instance1, parameter_types, traits::IsType};
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
		System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
		Staking: pallet_cf_staking::{Pallet, Call, Event<T>, Config<T>},
		Vaults: pallet_cf_vaults::<Instance1>::{Pallet, Call, Event<T>, Config},
		WitnessApi: pallet_cf_witness_api::{Pallet, Call},
		EthereumThresholdSigner: pallet_cf_threshold_signature::<Instance1>::{Pallet, Call, Event<T>, Storage, Origin<T>},
		EthereumBroadcaster: pallet_cf_broadcast::<Instance1>::{Pallet, Call, Event<T>, Storage},
	}
);

impl_mock_witnesser_for_account_and_call_types!(u64, Call, u64);

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

impl_mock_stake_transfer!(u64, u128);

pub struct AccountIdU64(u64);

impl AsRef<[u8; 32]> for AccountIdU64 {
	fn as_ref(&self) -> &[u8; 32] {
		unimplemented!()
	}
}

impl From<u64> for AccountIdU64 {
	fn from(x: u64) -> Self {
		Self(x)
	}
}

impl From<AccountIdU64> for u64 {
	fn from(x: AccountIdU64) -> Self {
		x.0
	}
}

impl IsType<u64> for AccountIdU64 {
	fn from_ref(_t: &u64) -> &Self {
		unimplemented!()
	}

	fn into_ref(&self) -> &u64 {
		&self.0
	}

	fn from_mut(_t: &mut u64) -> &mut Self {
		unimplemented!()
	}

	fn into_mut(&mut self) -> &mut u64 {
		&mut self.0
	}
}

impl pallet_cf_staking::Config for Test {
	type Event = Event;
	type ThresholdCallable = Call;
	type Balance = u128;
	type Flip = MockStakeTransfer;
	type TimeSource = cf_traits::mocks::time_source::Mock;
	type StakerId = AccountIdU64;
	type ReplayProtectionProvider = MockEthReplayProtectionProvider<Ethereum>;
	type ThresholdSigner = EthereumThresholdSigner;
	type EnsureThresholdSigned = NeverFailingOriginCheck<Self>;
	type WeightInfo = ();
	type EnsureGovernance = NeverFailingOriginCheck<Self>;
	type RegisterClaim = EthereumApi;
	type EthEnvironmentProvider = MockEthEnvironmentProvider;
}

type Amount = u128;
type ValidatorId = u64;

impl Chainflip for Test {
	type Amount = Amount;
	type ValidatorId = ValidatorId;
	type EnsureWitnessed = NeverFailingOriginCheck<Self>;
	type KeyId = Vec<u8>;
	type Call = Call;
	type EpochInfo = MockEpochInfo;
}

cf_traits::impl_mock_signer_nomination!(u64);

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Encode, Decode)]
pub struct MockRuntimeOffence;

impl From<pallet_cf_threshold_signature::PalletOffence> for MockRuntimeOffence {
	fn from(_: pallet_cf_threshold_signature::PalletOffence) -> Self {
		Self
	}
}

impl From<pallet_cf_broadcast::PalletOffence> for MockRuntimeOffence {
	fn from(_: pallet_cf_broadcast::PalletOffence) -> Self {
		Self
	}
}

impl From<pallet_cf_vaults::PalletOffence> for MockRuntimeOffence {
	fn from(_: pallet_cf_vaults::PalletOffence) -> Self {
		Self
	}
}

pub type MockOffenceReporter =
	cf_traits::mocks::offence_reporting::MockOffenceReporter<ValidatorId, MockRuntimeOffence>;

parameter_types! {
	pub const ThresholdFailureTimeout: <Test as frame_system::Config>::BlockNumber = 10;
	pub const CeremonyRetryDelay: <Test as frame_system::Config>::BlockNumber = 1;
}

impl pallet_cf_threshold_signature::Config<Instance1> for Test {
	type Event = Event;
	type Offence = MockRuntimeOffence;
	type RuntimeOrigin = Origin;
	type ThresholdCallable = Call;
	type TargetChain = Ethereum;
	type SignerNomination = MockSignerNomination;
	type KeyProvider = MockKeyProvider<Ethereum, <Self as Chainflip>::KeyId>;
	type OffenceReporter = MockOffenceReporter;
	type CeremonyIdProvider = MockCeremonyIdProvider<u64>;
	type ThresholdFailureTimeout = ThresholdFailureTimeout;
	type CeremonyRetryDelay = CeremonyRetryDelay;
	type Weights = ();
}

parameter_types! {
	pub const SigningTimeout: <Test as frame_system::Config>::BlockNumber = 10;
	pub const TransmissionTimeout: <Test as frame_system::Config>::BlockNumber = 10;
	pub const MaximumAttempts: u32 = 3;
}

impl pallet_cf_broadcast::Config<Instance1> for Test {
	type Event = Event;
	type Call = Call;
	type Offence = MockRuntimeOffence;
	type TargetChain = Ethereum;
	type ApiCall = EthereumApi;
	type TransactionBuilder = MockTransactionBuilder<Ethereum, EthereumApi>;
	type ThresholdSigner = EthereumThresholdSigner;
	type SignerNomination = MockSignerNomination;
	type OffenceReporter = MockOffenceReporter;
	type EnsureThresholdSigned = NeverFailingOriginCheck<Self>;
	type SigningTimeout = SigningTimeout;
	type TransmissionTimeout = TransmissionTimeout;
	type MaximumAttempts = MaximumAttempts;
	type WeightInfo = ();
}

parameter_types! {
	pub const KeygenResponseGracePeriod: u64 = 25; // 25 * 6 == 150 seconds
}

impl pallet_cf_vaults::Config<Instance1> for Test {
	type Event = Event;
	type Offence = MockRuntimeOffence;
	type Chain = Ethereum;
	type OffenceReporter = MockOffenceReporter;
	type CeremonyIdProvider = MockCeremonyIdProvider<u64>;
	type WeightInfo = pallet_cf_vaults::weights::PalletWeight<Test>;
	type KeygenResponseGracePeriod = KeygenResponseGracePeriod;
	type ApiCall = EthereumApi;
	type Broadcaster = EthereumBroadcaster;
	type EthEnvironmentProvider = MockEthEnvironmentProvider;
	type ReplayProtectionProvider = MockEthReplayProtectionProvider<Ethereum>;
}

impl pallet_cf_witness_api::Config for Test {
	type Call = Call;
	type Witnesser = MockWitnesser;
	type WeightInfoWitnesser = ();
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	system::GenesisConfig::default().build_storage::<Test>().unwrap().into()
}
