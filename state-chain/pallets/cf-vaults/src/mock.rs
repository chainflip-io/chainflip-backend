use std::cell::RefCell;

use frame_support::{construct_runtime, parameter_types, traits::UnfilteredDispatchable};
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};

use crate as pallet_cf_vaults;

use super::*;
use cf_chains::{eth, ChainCrypto};
use cf_traits::Chainflip;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<MockRuntime>;
type Block = frame_system::mocking::MockBlock<MockRuntime>;

type ValidatorId = u64;

thread_local! {
	pub static BAD_VALIDATORS: RefCell<Vec<ValidatorId>> = RefCell::new(vec![]);
}

construct_runtime!(
	pub enum MockRuntime where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
		VaultsPallet: pallet_cf_vaults::{Pallet, Call, Storage, Event<T>, Config},
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
}

impl frame_system::Config for MockRuntime {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
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
	type DbWeight = ();
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
}

parameter_types! {}

cf_traits::impl_mock_offline_conditions!(u64);

impl Chainflip for MockRuntime {
	type KeyId = Vec<u8>;
	type ValidatorId = ValidatorId;
	type Amount = u128;
	type Call = Call;
	type EnsureWitnessed = cf_traits::mocks::ensure_origin_mock::NeverFailingOriginCheck<Self>;
	type EpochInfo = cf_traits::mocks::epoch_info::MockEpochInfo;
}

pub struct MockRotationHandler;

impl VaultRotationHandler for MockRotationHandler {
	type ValidatorId = ValidatorId;
	fn vault_rotation_aborted() {}
}

pub struct MockCallback;

impl UnfilteredDispatchable for MockCallback {
	type Origin = Origin;

	fn dispatch_bypass_filter(
		self,
		_origin: Self::Origin,
	) -> frame_support::dispatch::DispatchResultWithPostInfo {
		Ok(().into())
	}
}

pub struct MockEthSigningContext;

impl From<eth::set_agg_key_with_agg_key::SetAggKeyWithAggKey> for MockEthSigningContext {
	fn from(_: eth::set_agg_key_with_agg_key::SetAggKeyWithAggKey) -> Self {
		MockEthSigningContext
	}
}

impl SigningContext<MockRuntime> for MockEthSigningContext {
	type Chain = Ethereum;
	type Callback = MockCallback;
	type ThresholdSignatureOrigin = Origin;

	fn get_payload(&self) -> <Self::Chain as ChainCrypto>::Payload {
		Default::default()
	}

	fn resolve_callback(
		&self,
		_signature: <Self::Chain as ChainCrypto>::ThresholdSignature,
	) -> Self::Callback {
		MockCallback
	}
}

pub struct MockThresholdSigner;

impl ThresholdSigner<MockRuntime> for MockThresholdSigner {
	type Context = MockEthSigningContext;

	fn request_signature(_context: Self::Context) -> u64 {
		0
	}
}

impl pallet_cf_vaults::Config for MockRuntime {
	type Event = Event;
	type RotationHandler = MockRotationHandler;
	type OfflineReporter = MockOfflineReporter;
	type SigningContext = MockEthSigningContext;
	type ThresholdSigner = MockThresholdSigner;
	type WeightInfo = ();
}

pub const ALICE: <MockRuntime as frame_system::Config>::AccountId = 123u64;
pub const BOB: <MockRuntime as frame_system::Config>::AccountId = 456u64;
pub const CHARLIE: <MockRuntime as frame_system::Config>::AccountId = 789u64;
pub const GENESIS_ETHEREUM_AGG_PUB_KEY: [u8; 33] = [0x02; 33];

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
	let config = GenesisConfig {
		system: Default::default(),
		vaults_pallet: VaultsPalletConfig {
			ethereum_vault_key: GENESIS_ETHEREUM_AGG_PUB_KEY.to_vec(),
		},
	};

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
