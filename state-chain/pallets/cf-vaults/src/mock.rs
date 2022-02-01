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
use cf_chains::{
	eth::{self, SchnorrVerificationComponents},
	ChainCrypto,
};
use cf_traits::{AsyncResult, Chainflip};

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

pub struct MockThresholdSigner;

impl ThresholdSigner<Ethereum> for MockThresholdSigner {
	type RequestId = u32;
	type Error = &'static str;
	type Callback = MockCallback;

	fn request_signature(_: <Ethereum as ChainCrypto>::Payload) -> Self::RequestId {
		Default::default()
	}

	fn register_callback(_: Self::RequestId, _: Self::Callback) -> Result<(), Self::Error> {
		Ok(())
	}

	fn signature_result(
		_: Self::RequestId,
	) -> cf_traits::AsyncResult<<Ethereum as ChainCrypto>::ThresholdSignature> {
		AsyncResult::Ready(SchnorrVerificationComponents::default())
	}
}

parameter_types! {
	pub const KeygenResponseGracePeriod: u64 = 25; // 25 * 6 == 150 seconds
}

impl pallet_cf_vaults::Config for MockRuntime {
	type Event = Event;
	type Chain = Ethereum;
	type RotationHandler = MockRotationHandler;
	type OfflineReporter = MockOfflineReporter;
	type ThresholdSigner = MockThresholdSigner;
	type SetAggKeyWithAggKey = eth::set_agg_key_with_agg_key::SetAggKeyWithAggKey;
	type WeightInfo = ();
	type KeygenResponseGracePeriod = KeygenResponseGracePeriod;
}

pub const ALICE: <MockRuntime as frame_system::Config>::AccountId = 123u64;
pub const BOB: <MockRuntime as frame_system::Config>::AccountId = 456u64;
pub const CHARLIE: <MockRuntime as frame_system::Config>::AccountId = 789u64;
pub const GENESIS_ETHEREUM_AGG_PUB_KEY: [u8; 33] = [0x02; 33];

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
	let config = GenesisConfig {
		system: Default::default(),
		vaults_pallet: VaultsPalletConfig {
			vault_key: GENESIS_ETHEREUM_AGG_PUB_KEY.to_vec(),
			deployment_block: 0,
		},
	};

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
