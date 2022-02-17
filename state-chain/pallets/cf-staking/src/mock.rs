use crate as pallet_cf_staking;
use cf_chains::{
	eth, eth::register_claim::RegisterClaim, AlwaysVerifiesCoin, ChainCrypto, Ethereum,
};
use cf_traits::{impl_mock_waived_fees, WaivedFees};
use codec::{Decode, Encode};
use frame_support::{instances::Instance1, parameter_types};
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

use cf_traits::{
	mocks::{ensure_origin_mock::NeverFailingOriginCheck, time_source},
	Chainflip, NonceProvider, SigningContext,
};

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
		Flip: pallet_cf_flip::{Pallet, Call, Config<T>, Storage, Event<T>},
		Signer: pallet_cf_threshold_signature::<Instance1>::{Pallet, Call, Storage, Event<T>, Origin<T>},
		Staking: pallet_cf_staking::{Pallet, Call, Config<T>, Storage, Event<T>},
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
	pub const ClaimTTL: Duration = Duration::from_secs(10);
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
}

impl Chainflip for Test {
	type KeyId = Vec<u8>;
	type ValidatorId = AccountId;
	type Amount = u128;
	type Call = Call;
	type EnsureWitnessed = MockEnsureWitnessed;
	type EpochInfo = MockEpochInfo;
}

cf_traits::impl_mock_signer_nomination!(AccountId);
cf_traits::impl_mock_offline_conditions!(AccountId);

pub struct MockKeyProvider;

impl cf_traits::KeyProvider<AlwaysVerifiesCoin> for MockKeyProvider {
	type KeyId = Vec<u8>;

	fn current_key_id() -> Self::KeyId {
		Default::default()
	}

	fn current_key() -> <AlwaysVerifiesCoin as ChainCrypto>::AggKey {
		vec![]
	}
}

parameter_types! {
	pub const ThresholdFailureTimeout: <Test as frame_system::Config>::BlockNumber = 10;
	pub const CeremonyRetryDelay: <Test as frame_system::Config>::BlockNumber = 1;
}

impl pallet_cf_threshold_signature::Config<Instance1> for Test {
	type Event = Event;
	type TargetChain = AlwaysVerifiesCoin;
	type SigningContext = ClaimSigningContext;
	type SignerNomination = MockSignerNomination;
	type KeyProvider = MockKeyProvider;
	type OfflineReporter = MockOfflineReporter;
	type ThresholdFailureTimeout = ThresholdFailureTimeout;
	type CeremonyRetryDelay = CeremonyRetryDelay;
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
cf_traits::impl_mock_witnesser_for_account_and_call_types!(AccountId, Call, u64);
cf_traits::impl_mock_epoch_info!(AccountId, u128, u32);
cf_traits::impl_mock_stake_transfer!(AccountId, u128);

pub const NONCE: u64 = 42;

impl NonceProvider<Ethereum> for Test {
	fn next_nonce() -> cf_traits::Nonce {
		NONCE
	}
}

// Mock SigningContext

pub const ETH_DUMMY_SIG: eth::SchnorrVerificationComponents =
	eth::SchnorrVerificationComponents { s: [0xcf; 32], k_times_g_addr: [0xcf; 20] };

#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode)]
pub struct ClaimSigningContext(RegisterClaim);

impl From<RegisterClaim> for ClaimSigningContext {
	fn from(r: RegisterClaim) -> Self {
		ClaimSigningContext(r)
	}
}

impl SigningContext<Test> for ClaimSigningContext {
	type Chain = AlwaysVerifiesCoin;
	type Callback = pallet_cf_staking::Call<Test>;
	type ThresholdSignatureOrigin = pallet_cf_threshold_signature::Origin<Test, Instance1>;

	fn get_payload(&self) -> <Self::Chain as ChainCrypto>::Payload {
		Default::default()
	}

	fn resolve_callback(
		&self,
		_signature: <Self::Chain as ChainCrypto>::ThresholdSignature,
	) -> Self::Callback {
		pallet_cf_staking::Call::<Test>::post_claim_signature(self.0.node_id.into(), ETH_DUMMY_SIG)
	}
}

impl pallet_cf_staking::Config for Test {
	type Event = Event;
	type TimeSource = time_source::Mock;
	type ClaimTTL = ClaimTTL;
	type Balance = u128;
	type Flip = Flip;
	type WeightInfo = ();
	type StakerId = AccountId;
	type NonceProvider = Self;
	type SigningContext = ClaimSigningContext;
	type ThresholdSigner = Signer;
	type EnsureThresholdSigned = NeverFailingOriginCheck<Self>;
	type EnsureGovernance = NeverFailingOriginCheck<Self>;
}

pub const ALICE: AccountId = AccountId32::new([0xa1; 32]);
pub const BOB: AccountId = AccountId32::new([0xb0; 32]);
pub const MIN_STAKE: u128 = 10;
// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let config = GenesisConfig {
		system: Default::default(),
		flip: FlipConfig { total_issuance: 1_000 },
		staking: StakingConfig { genesis_stakers: vec![], minimum_stake: MIN_STAKE },
	};
	MockSignerNomination::set_candidates(vec![ALICE]);

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
