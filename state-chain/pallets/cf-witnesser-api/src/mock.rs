use crate as pallet_cf_witness_api;
use codec::{Decode, Encode};
use std::time::Duration;

use cf_chains::{
	eth::{register_claim::RegisterClaim, set_agg_key_with_agg_key::SetAggKeyWithAggKey},
	Ethereum,
};
use cf_traits::{
	impl_mock_ensure_witnessed_for_origin, impl_mock_stake_transfer,
	impl_mock_witnesser_for_account_and_call_types, mocks::key_provider::MockKeyProvider,
	Chainflip, NonceProvider, VaultRotationHandler,
};
use frame_support::{instances::Instance0, parameter_types, traits::IsType};
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
		Vaults: pallet_cf_vaults::{Module, Call, Event<T>, Config},
		WitnessApi: pallet_cf_witness_api::{Module, Call},
		EthereumThresholdSigner: pallet_cf_threshold_signature::<Instance0>::{Module, Call, Event<T>, Storage},
		EthereumBroadcaster: pallet_cf_broadcast::<Instance0>::{Module, Call, Event<T>, Storage},
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

impl NonceProvider<Ethereum> for Test {
	fn next_nonce() -> cf_traits::Nonce {
		42
	}
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Encode, Decode)]
pub struct MockSigningContext;

impl From<RegisterClaim> for MockSigningContext {
	fn from(_: RegisterClaim) -> Self {
		unimplemented!()
	}
}

impl From<SetAggKeyWithAggKey> for MockSigningContext {
	fn from(_: SetAggKeyWithAggKey) -> Self {
		unimplemented!()
	}
}

impl cf_traits::SigningContext<Test> for MockSigningContext {
	type Chain = Ethereum;
	type Payload = ();
	type Signature = ();
	type Callback = Call;

	fn get_payload(&self) -> Self::Payload {
		()
	}

	fn resolve_callback(&self, _signature: Self::Signature) -> Self::Callback {
		Call::System(frame_system::Call::remark(b"Hello".to_vec()))
	}
}

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
	type Balance = u128;
	type Flip = MockStakeTransfer;
	type EpochInfo = cf_traits::mocks::epoch_info::Mock;
	type TimeSource = cf_traits::mocks::time_source::Mock;
	type MinClaimTTL = MinClaimTTL;
	type ClaimTTL = ClaimTTL;
	type StakerId = AccountIdU64;
	type NonceProvider = Self;
	type SigningContext = MockSigningContext;
	type ThresholdSigner = EthereumThresholdSigner;
	type WeightInfo = ();
}

type Amount = u64;
type ValidatorId = u64;

impl Chainflip for Test {
	type Amount = Amount;
	type ValidatorId = ValidatorId;
	type EnsureWitnessed = MockEnsureWitnessed;
	type KeyId = Vec<u8>;
	type Call = Call;
}

cf_traits::impl_mock_signer_nomination!(u64);
cf_traits::impl_mock_offline_conditions!(u64);

impl pallet_cf_threshold_signature::Config<Instance0> for Test {
	type Event = Event;
	type TargetChain = Ethereum;
	type SigningContext = MockSigningContext;
	type SignerNomination = MockSignerNomination;
	type KeyProvider = MockKeyProvider<Ethereum, <Self as Chainflip>::KeyId>;
	type OfflineReporter = MockOfflineReporter;
}

pub struct MockBroadcastConfig;

impl pallet_cf_broadcast::BroadcastConfig<Test> for MockBroadcastConfig {
	type Chain = Ethereum;
	type UnsignedTransaction = ();
	type SignedTransaction = ();
	type TransactionHash = ();

	fn verify_transaction(
		_signer: &<Test as Chainflip>::ValidatorId,
		_unsigned_tx: &Self::UnsignedTransaction,
		_signed_tx: &Self::SignedTransaction,
	) -> Option<()> {
		Some(())
	}
}

parameter_types! {
	pub const SigningTimeout: <Test as frame_system::Config>::BlockNumber = 10;
	pub const TransmissionTimeout: <Test as frame_system::Config>::BlockNumber = 10;
}

impl pallet_cf_broadcast::Config<Instance0> for Test {
	type Event = Event;
	type TargetChain = Ethereum;
	type BroadcastConfig = MockBroadcastConfig;
	type SignerNomination = MockSignerNomination;
	type OfflineReporter = MockOfflineReporter;
	type SigningTimeout = SigningTimeout;
	type TransmissionTimeout = TransmissionTimeout;
}

impl VaultRotationHandler for Test {
	type ValidatorId = ValidatorId;

	fn vault_rotation_aborted() {}
	fn penalise(_bad_validators: &[Self::ValidatorId]) {}
}

impl pallet_cf_vaults::Config for Test {
	type Event = Event;
	type RotationHandler = Self;
	type EpochInfo = cf_traits::mocks::epoch_info::Mock;
	type OfflineReporter = MockOfflineReporter;
	type SigningContext = MockSigningContext;
	type ThresholdSigner = EthereumThresholdSigner;
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
