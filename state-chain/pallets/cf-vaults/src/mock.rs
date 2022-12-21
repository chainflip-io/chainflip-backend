use std::cell::RefCell;

use cf_primitives::BroadcastId;
use frame_support::{
	construct_runtime, parameter_types, traits::UnfilteredDispatchable, StorageHasher,
};
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};

use crate as pallet_cf_vaults;

use super::*;
use cf_chains::{eth, mocks::MockEthereum, ApiCall, ChainCrypto, ReplayProtectionProvider};
use cf_traits::{
	mocks::{
		ceremony_id_provider::MockCeremonyIdProvider, ensure_origin_mock::NeverFailingOriginCheck,
		epoch_info::MockEpochInfo, eth_replay_protection_provider::MockEthReplayProtectionProvider,
		system_state_info::MockSystemStateInfo, threshold_signer::MockThresholdSigner,
	},
	Chainflip,
};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<MockRuntime>;
type Block = frame_system::mocking::MockBlock<MockRuntime>;

pub type ValidatorId = u64;

thread_local! {
	pub static BAD_VALIDATORS: RefCell<Vec<ValidatorId>> = RefCell::new(vec![]);
	pub static CURRENT_SYSTEM_STATE: RefCell<SystemState> = RefCell::new(SystemState::Normal);

}

construct_runtime!(
	pub enum MockRuntime where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system,
		VaultsPallet: pallet_cf_vaults,
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
}

#[derive(Clone, Eq, PartialEq, Copy, Debug)]
pub enum SystemState {
	Normal,
	Maintenance,
}

// TODO: Unify with staking pallet mock
pub const ETH_DUMMY_SIG: eth::SchnorrVerificationComponents =
	eth::SchnorrVerificationComponents { s: [0xcf; 32], k_times_g_address: [0xcf; 20] };

// do not know how to solve this mock
pub struct MockSystemStateManager;

impl MockSystemStateManager {
	pub fn set_system_state(state: SystemState) {
		CURRENT_SYSTEM_STATE.with(|cell| {
			*cell.borrow_mut() = state;
		});
	}
}

impl SystemStateManager for MockSystemStateManager {
	fn activate_maintenance_mode() {
		Self::set_system_state(SystemState::Maintenance);
	}
}

impl MockSystemStateManager {
	pub fn get_current_system_state() -> SystemState {
		CURRENT_SYSTEM_STATE.with(|cell| *cell.borrow())
	}
}

impl frame_system::Config for MockRuntime {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Index = u64;
	type BlockNumber = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = Header;
	type RuntimeEvent = RuntimeEvent;
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
	type MaxConsumers = frame_support::traits::ConstU32<5>;
}

parameter_types! {}

impl Chainflip for MockRuntime {
	type KeyId = Vec<u8>;
	type ValidatorId = ValidatorId;
	type Amount = u128;
	type RuntimeCall = RuntimeCall;
	type EnsureWitnessed = cf_traits::mocks::ensure_origin_mock::NeverFailingOriginCheck<Self>;
	type EnsureWitnessedAtCurrentEpoch =
		cf_traits::mocks::ensure_origin_mock::NeverFailingOriginCheck<Self>;
	type EpochInfo = MockEpochInfo;
	type SystemState = MockSystemStateInfo;
}

pub struct MockCallback;

impl UnfilteredDispatchable for MockCallback {
	type RuntimeOrigin = RuntimeOrigin;

	fn dispatch_bypass_filter(
		self,
		_origin: Self::Origin,
	) -> frame_support::dispatch::DispatchResultWithPostInfo {
		Ok(().into())
	}
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct MockSetAggKeyWithAggKey {
	nonce: <MockEthereum as ChainAbi>::ReplayProtection,
	new_key: <MockEthereum as ChainCrypto>::AggKey,
}

impl SetAggKeyWithAggKey<MockEthereum> for MockSetAggKeyWithAggKey {
	fn new_unsigned(
		old_key: Option<<MockEthereum as ChainCrypto>::AggKey>,
		new_key: <MockEthereum as ChainCrypto>::AggKey,
	) -> Result<Self, ()> {
		old_key.ok_or(())?;
		Ok(Self { nonce: MockEthReplayProtectionProvider::replay_protection(), new_key })
	}
}

impl ApiCall<MockEthereum> for MockSetAggKeyWithAggKey {
	fn threshold_signature_payload(&self) -> <MockEthereum as ChainCrypto>::Payload {
		unimplemented!()
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

pub struct MockVaultTransitionHandler;
impl VaultTransitionHandler<MockEthereum> for MockVaultTransitionHandler {
	fn on_new_vault() {}
}

pub struct MockBroadcaster;

impl MockBroadcaster {
	pub fn send_broadcast() {
		storage::hashed::put(&<Twox64Concat as StorageHasher>::hash, b"MockBroadcaster", &());
	}

	pub fn broadcast_sent() -> bool {
		storage::hashed::exists(&<Twox64Concat as StorageHasher>::hash, b"MockBroadcaster")
	}
}

impl Broadcaster<MockEthereum> for MockBroadcaster {
	type ApiCall = MockSetAggKeyWithAggKey;

	fn threshold_sign_and_broadcast(_api_call: Self::ApiCall) -> BroadcastId {
		Self::send_broadcast();
		1
	}
}

parameter_types! {
	pub const KeygenResponseGracePeriod: u64 = 25;
}

pub type MockOffenceReporter =
	cf_traits::mocks::offence_reporting::MockOffenceReporter<ValidatorId, PalletOffence>;

impl pallet_cf_vaults::Config for MockRuntime {
	type RuntimeEvent = RuntimeEvent;
	type Offence = PalletOffence;
	type Chain = MockEthereum;
	type RuntimeCall = RuntimeCall;
	type AccountRoleRegistry = ();
	type EnsureGovernance = NeverFailingOriginCheck<Self>;
	type EnsureThresholdSigned = NeverFailingOriginCheck<Self>;
	type ThresholdSigner = MockThresholdSigner<MockEthereum, Call>;
	type OffenceReporter = MockOffenceReporter;
	type SetAggKeyWithAggKey = MockSetAggKeyWithAggKey;
	type VaultTransitionHandler = MockVaultTransitionHandler;
	type CeremonyIdProvider = MockCeremonyIdProvider<CeremonyId>;
	type WeightInfo = ();
	type Broadcaster = MockBroadcaster;
	type SystemStateManager = MockSystemStateManager;
}

pub const ALICE: <MockRuntime as frame_system::Config>::AccountId = 123u64;
pub const BOB: <MockRuntime as frame_system::Config>::AccountId = 456u64;
pub const CHARLIE: <MockRuntime as frame_system::Config>::AccountId = 789u64;
pub const GENESIS_AGG_PUB_KEY: [u8; 4] = *b"genk";
pub const NEW_AGG_PUB_KEY: [u8; 4] = *b"next";

pub const MOCK_KEYGEN_RESPONSE_TIMEOUT: u64 = 25;

fn test_ext_inner(key: Option<Vec<u8>>) -> sp_io::TestExternalities {
	let config = GenesisConfig {
		system: Default::default(),
		vaults_pallet: VaultsPalletConfig {
			vault_key: key,
			deployment_block: 0,
			keygen_response_timeout: MOCK_KEYGEN_RESPONSE_TIMEOUT,
		},
	};

	let authorities = vec![ALICE, BOB, CHARLIE];
	MockEpochInfo::set_epoch(GENESIS_EPOCH);
	MockEpochInfo::set_epoch_authority_count(GENESIS_EPOCH, authorities.len() as AuthorityCount);
	MockEpochInfo::set_authorities(authorities);

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
	test_ext_inner(Some(GENESIS_AGG_PUB_KEY.to_vec()))
}

pub(crate) fn new_test_ext_no_key() -> sp_io::TestExternalities {
	test_ext_inner(None)
}
