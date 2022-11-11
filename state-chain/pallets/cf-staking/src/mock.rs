use crate as pallet_cf_staking;
use cf_chains::{
	eth::{self, api::EthereumReplayProtection, Ethereum},
	ChainAbi, ChainCrypto,
};
use cf_primitives::{AuthorityCount, CeremonyId};
use cf_traits::{
	impl_mock_waived_fees, mocks::system_state_info::MockSystemStateInfo, AsyncResult,
	ThresholdSigner, WaivedFees,
};
use frame_support::{dispatch::DispatchResultWithPostInfo, parameter_types, traits::ConstU64};
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
	AccountId32, BuildStorage,
};
use sp_std::collections::btree_set::BTreeSet;
use std::time::Duration;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;
// Use a realistic account id for compatibility with `RegisterClaim`.
type AccountId = AccountId32;

use cf_traits::{
	mocks::{
		ensure_origin_mock::NeverFailingOriginCheck,
		eth_environment_provider::MockEthEnvironmentProvider, time_source,
	},
	Chainflip, ReplayProtectionProvider,
};

impl pallet_cf_account_roles::Config for Test {
	type Event = Event;
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
	type Origin = Origin;
	type Call = Call;
	type Index = u64;
	type BlockNumber = u64;
	type Hash = sp_core::H256;
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

impl Chainflip for Test {
	type KeyId = Vec<u8>;
	type ValidatorId = AccountId;
	type Amount = u128;
	type Call = Call;
	type EnsureWitnessed = MockEnsureWitnessed;
	type EnsureWitnessedAtCurrentEpoch = MockEnsureWitnessed;
	type EpochInfo = MockEpochInfo;
	type SystemState = MockSystemStateInfo;
}

parameter_types! {
	pub const CeremonyRetryDelay: <Test as frame_system::Config>::BlockNumber = 1;
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

cf_traits::impl_mock_ensure_witnessed_for_origin!(Origin);
cf_traits::impl_mock_epoch_info!(AccountId, u128, u32, AuthorityCount);
cf_traits::impl_mock_stake_transfer!(AccountId, u128);

pub const FAKE_KEYMAN_ADDR: [u8; 20] = [0xcf; 20];
pub const CHAIN_ID: u64 = 31337;
pub const COUNTER: u64 = 42;

impl ReplayProtectionProvider<Ethereum> for Test {
	fn replay_protection() -> <Ethereum as ChainAbi>::ReplayProtection {
		EthereumReplayProtection {
			key_manager_address: FAKE_KEYMAN_ADDR,
			chain_id: CHAIN_ID,
			nonce: COUNTER,
		}
	}
}

pub struct MockThresholdSigner;

thread_local! {
	pub static SIGNATURE_REQUESTS: RefCell<Vec<<Ethereum as ChainCrypto>::Payload>> = RefCell::new(vec![]);
}

impl MockThresholdSigner {
	pub fn received_requests() -> Vec<<Ethereum as ChainCrypto>::Payload> {
		SIGNATURE_REQUESTS.with(|cell| cell.borrow().clone())
	}

	pub fn on_signature_ready(account_id: &AccountId) -> DispatchResultWithPostInfo {
		Staking::post_claim_signature(Origin::root(), account_id.clone(), 0)
	}
}

impl ThresholdSigner<Ethereum> for MockThresholdSigner {
	type RequestId = u32;
	type Error = &'static str;
	type Callback = Call;
	type KeyId = <Test as Chainflip>::KeyId;
	type ValidatorId = AccountId;

	fn request_signature(
		payload: <Ethereum as ChainCrypto>::Payload,
	) -> (Self::RequestId, CeremonyId) {
		SIGNATURE_REQUESTS.with(|cell| cell.borrow_mut().push(payload));
		(0, 1)
	}

	fn request_keygen_verification_signature(
		payload: <Ethereum as ChainCrypto>::Payload,
		_key_id: Self::KeyId,
		_participants: BTreeSet<Self::ValidatorId>,
	) -> (Self::RequestId, CeremonyId) {
		Self::request_signature(payload)
	}

	fn register_callback(_: Self::RequestId, _: Self::Callback) -> Result<(), Self::Error> {
		Ok(())
	}

	fn signature_result(
		_: Self::RequestId,
	) -> cf_traits::AsyncResult<
		Result<<Ethereum as ChainCrypto>::ThresholdSignature, Vec<Self::ValidatorId>>,
	> {
		AsyncResult::Ready(Ok(ETH_DUMMY_SIG))
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn insert_signature(
		_request_id: Self::RequestId,
		_signature: <Ethereum as ChainCrypto>::ThresholdSignature,
	) {
		// do nothing, the mock impl of signature_result doesn't take from any storage
		// so we don't need to insert any storage.
	}
}

// The dummy signature can't be Default - this would be interpreted as no signature.
pub const ETH_DUMMY_SIG: eth::SchnorrVerificationComponents =
	eth::SchnorrVerificationComponents { s: [0xcf; 32], k_times_g_address: [0xcf; 20] };

pub const CLAIM_DELAY_BUFFER_SECS: u64 = 10;

impl pallet_cf_staking::Config for Test {
	type Event = Event;
	type TimeSource = time_source::Mock;
	type Balance = u128;
	type AccountRoleRegistry = ();
	type Flip = Flip;
	type WeightInfo = ();
	type StakerId = AccountId;
	type ReplayProtectionProvider = Self;
	type ThresholdSigner = MockThresholdSigner;
	type ThresholdCallable = Call;
	type EnsureThresholdSigned = NeverFailingOriginCheck<Self>;
	type EnsureGovernance = NeverFailingOriginCheck<Self>;
	type ClaimDelayBufferSeconds = ConstU64<CLAIM_DELAY_BUFFER_SECS>;
	type RegisterClaim = eth::api::EthereumApi<()>;
	type EthEnvironmentProvider = MockEthEnvironmentProvider;
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
		},
	};

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
