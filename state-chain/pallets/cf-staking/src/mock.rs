use crate as pallet_cf_staking;
use cf_chains::{
	eth::{register_claim::RegisterClaim, ChainflipContractCall, SchnorrVerificationComponents},
	Ethereum,
};
use codec::{Decode, Encode};
use frame_support::{instances::Instance0, parameter_types, traits::EnsureOrigin};
use pallet_cf_flip;
use sp_core::H256;
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
	mocks::{key_provider, time_source},
	Chainflip, NonceProvider, SigningContext,
};

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Module, Call, Config, Storage, Event<T>},
		Flip: pallet_cf_flip::{Module, Call, Config<T>, Storage, Event<T>},
		Signer: pallet_cf_signing::<Instance0>::{Module, Call, Storage, Event<T>},
		Staking: pallet_cf_staking::{Module, Call, Config<T>, Storage, Event<T>},
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
	pub const MinClaimTTL: Duration = Duration::from_secs(4);
	pub const ClaimTTL: Duration = Duration::from_secs(10);
}

impl frame_system::Config for Test {
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
}

impl Chainflip for Test {
	type KeyId = u32;
	type ValidatorId = AccountId;
	type Amount = u128;
	type Call = Call;
	type EnsureWitnessed = MockEnsureWitnessed;
}

cf_traits::impl_mock_signer_nomination!(AccountId);
cf_traits::impl_mock_offline_conditions!(AccountId);

impl pallet_cf_signing::Config<Instance0> for Test {
	type Event = Event;
	type TargetChain = Ethereum;
	type SigningContext = ClaimSigningContext;
	type SignerNomination = MockSignerNomination;
	type KeyProvider = key_provider::MockKeyProvider<Ethereum, Self::KeyId>;
	type OfflineReporter = MockOfflineReporter;
}

parameter_types! {
	pub const ExistentialDeposit: u128 = 10;
}

pub struct MockEnsureGovernance;

impl EnsureOrigin<Origin> for MockEnsureGovernance {
	type Success = ();

	fn try_origin(_o: Origin) -> Result<Self::Success, Origin> {
		Ok(().into())
	}
}

parameter_types! {
	pub const BlocksPerDay: u64 = 14400;
}

impl pallet_cf_flip::Config for Test {
	type Event = Event;
	type Balance = u128;
	type ExistentialDeposit = ExistentialDeposit;
	type EnsureGovernance = MockEnsureGovernance;
	type BlocksPerDay = BlocksPerDay;
}

cf_traits::impl_mock_ensure_witnessed_for_origin!(Origin);
cf_traits::impl_mock_witnesser_for_account_and_call_types!(AccountId, Call);
cf_traits::impl_mock_epoch_info!(AccountId, u128, u32);

pub const NONCE: u64 = 42;

impl NonceProvider for Test {
	fn next_nonce(_identifier: cf_traits::NonceIdentifier) -> cf_traits::Nonce {
		NONCE
	}
}

// Mock SigningContext

#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode)]
pub struct ClaimSigningContext(RegisterClaim);

impl From<RegisterClaim> for ClaimSigningContext {
	fn from(r: RegisterClaim) -> Self {
		ClaimSigningContext(r)
	}
}

impl SigningContext<Test> for ClaimSigningContext {
	type Chain = Ethereum;
	type Payload = H256;
	type Signature = SchnorrVerificationComponents;
	type Callback = pallet_cf_staking::Call<Test>;

	fn get_payload(&self) -> Self::Payload {
		ChainflipContractCall::signing_payload(&self.0)
	}

	fn resolve_callback(&self, signature: Self::Signature) -> Self::Callback {
		pallet_cf_staking::Call::<Test>::post_claim_signature(self.0.node_id.into(), signature)
	}
}

impl pallet_cf_staking::Config for Test {
	type Event = Event;
	type EpochInfo = MockEpochInfo;
	type TimeSource = time_source::Mock;
	type MinClaimTTL = MinClaimTTL;
	type ClaimTTL = ClaimTTL;
	type Balance = u128;
	type Flip = Flip;
	type AccountId = AccountId;
	type NonceProvider = Self;
	type SigningContext = ClaimSigningContext;
	type ThresholdSigner = Signer;
}

pub const ALICE: AccountId = AccountId32::new([0xa1; 32]);
pub const BOB: AccountId = AccountId32::new([0xb0; 32]);

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let config = GenesisConfig {
		frame_system: Default::default(),
		pallet_cf_flip: Some(FlipConfig {
			total_issuance: 1_000,
		}),
		pallet_cf_staking: Some(StakingConfig {
			genesis_stakers: vec![],
		}),
	};
	MockSignerNomination::set_candidates(vec![ALICE]);

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
