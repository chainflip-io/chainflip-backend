#![cfg(test)]

use std::cell::RefCell;

use super::*;
use crate as pallet_cf_vaults;
use cf_chains::{
	eth,
	mocks::{MockAggKey, MockEthereum},
	ApiCall, SetAggKeyWithAggKeyError,
};
use cf_primitives::{BroadcastId, GENESIS_EPOCH};
use cf_traits::{
	impl_mock_callback, impl_mock_chainflip, impl_mock_runtime_safe_mode,
	mocks::threshold_signer::MockThresholdSigner, AccountRoleRegistry, GetBlockHeight,
};
use frame_support::{
	construct_runtime, parameter_types, traits::UnfilteredDispatchable, StorageHasher,
};
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<MockRuntime>;
type Block = frame_system::mocking::MockBlock<MockRuntime>;

pub type ValidatorId = u64;

thread_local! {
	pub static BAD_VALIDATORS: RefCell<Vec<ValidatorId>> = RefCell::new(vec![]);
	pub static SET_AGG_KEY_WITH_AGG_KEY_REQUIRED: RefCell<bool> = RefCell::new(true);
	pub static SLASHES: RefCell<Vec<u64>> = RefCell::new(Default::default());
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

pub const ETH_DUMMY_SIG: eth::SchnorrVerificationComponents =
	eth::SchnorrVerificationComponents { s: [0xcf; 32], k_times_g_address: [0xcf; 20] };

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

impl_mock_chainflip!(MockRuntime);
impl_mock_callback!(RuntimeOrigin);

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct MockSetAggKeyWithAggKey {
	old_key: <MockEthereum as ChainCrypto>::AggKey,
	new_key: <MockEthereum as ChainCrypto>::AggKey,
}

impl MockSetAggKeyWithAggKey {
	pub fn set_required(required: bool) {
		SET_AGG_KEY_WITH_AGG_KEY_REQUIRED.with(|cell| {
			*cell.borrow_mut() = required;
		});
	}
}

impl SetAggKeyWithAggKey<MockEthereum> for MockSetAggKeyWithAggKey {
	fn new_unsigned(
		old_key: Option<<MockEthereum as ChainCrypto>::AggKey>,
		new_key: <MockEthereum as ChainCrypto>::AggKey,
	) -> Result<Self, SetAggKeyWithAggKeyError> {
		if !SET_AGG_KEY_WITH_AGG_KEY_REQUIRED.with(|cell| *cell.borrow()) {
			return Err(SetAggKeyWithAggKeyError::NotRequired)
		}

		Ok(Self { old_key: old_key.ok_or(SetAggKeyWithAggKeyError::Failed)?, new_key })
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

	fn transaction_out_id(&self) -> <MockEthereum as ChainCrypto>::TransactionOutId {
		todo!()
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
	type Callback = MockCallback;

	fn threshold_sign_and_broadcast(
		_api_call: Self::ApiCall,
	) -> (BroadcastId, ThresholdSignatureRequestId) {
		Self::send_broadcast();
		(1, 2)
	}

	fn threshold_sign_and_broadcast_with_callback(
		_api_call: Self::ApiCall,
		_callback: Self::Callback,
	) -> (BroadcastId, ThresholdSignatureRequestId) {
		unimplemented!()
	}
}

parameter_types! {
	pub const KeygenResponseGracePeriod: u64 = 25;
}

pub type MockOffenceReporter =
	cf_traits::mocks::offence_reporting::MockOffenceReporter<ValidatorId, PalletOffence>;

pub struct MockSlasher;

impl MockSlasher {
	pub fn slash_count(validator_id: ValidatorId) -> usize {
		SLASHES.with(|slashes| slashes.borrow().iter().filter(|id| **id == validator_id).count())
	}
}

impl Slashing for MockSlasher {
	type AccountId = ValidatorId;
	type BlockNumber = u64;

	fn slash(validator_id: &Self::AccountId, _blocks: Self::BlockNumber) {
		// Count those slashes
		SLASHES.with(|count| {
			count.borrow_mut().push(*validator_id);
		});
	}

	fn slash_balance(account_id: &Self::AccountId, _amount: sp_runtime::Percent) {
		// Count those slashes
		SLASHES.with(|count| {
			count.borrow_mut().push(*account_id);
		});
	}
}

pub struct BlockHeightProvider;

pub const HANDOVER_ACTIVATION_BLOCK: u64 = 1337;

impl GetBlockHeight<MockEthereum> for BlockHeightProvider {
	fn get_block_height() -> u64 {
		HANDOVER_ACTIVATION_BLOCK
	}
}

impl_mock_runtime_safe_mode! { vault: PalletSafeMode }

impl pallet_cf_vaults::Config for MockRuntime {
	type RuntimeEvent = RuntimeEvent;
	type Offence = PalletOffence;
	type Chain = MockEthereum;
	type RuntimeCall = RuntimeCall;
	type EnsureThresholdSigned = NeverFailingOriginCheck<Self>;
	type ThresholdSigner = MockThresholdSigner<MockEthereum, RuntimeCall>;
	type OffenceReporter = MockOffenceReporter;
	type SetAggKeyWithAggKey = MockSetAggKeyWithAggKey;
	type VaultTransitionHandler = MockVaultTransitionHandler;
	type WeightInfo = ();
	type Broadcaster = MockBroadcaster;
	type SafeMode = MockRuntimeSafeMode;
	type Slasher = MockSlasher;
	type ChainTracking = BlockHeightProvider;
}

pub const ALICE: <MockRuntime as frame_system::Config>::AccountId = 123u64;
pub const BOB: <MockRuntime as frame_system::Config>::AccountId = 456u64;
pub const CHARLIE: <MockRuntime as frame_system::Config>::AccountId = 789u64;
pub const GENESIS_AGG_PUB_KEY: MockAggKey = MockAggKey(*b"genk");
pub const NEW_AGG_PUB_KEY_PRE_HANDOVER: MockAggKey = MockAggKey(*b"next");
pub const NEW_AGG_PUB_KEY_POST_HANDOVER: MockAggKey = MockAggKey(*b"hand");

pub const MOCK_KEYGEN_RESPONSE_TIMEOUT: u64 = 25;

fn test_ext_inner(vault_key: Option<MockAggKey>) -> sp_io::TestExternalities {
	let config = GenesisConfig {
		system: Default::default(),
		vaults_pallet: VaultsPalletConfig {
			vault_key,
			deployment_block: 0,
			keygen_response_timeout: MOCK_KEYGEN_RESPONSE_TIMEOUT,
		},
	};

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
		let authorities = BTreeSet::from([ALICE, BOB, CHARLIE]);
		for id in &authorities {
			<MockAccountRoleRegistry as AccountRoleRegistry<MockRuntime>>::register_as_validator(
				id,
			)
			.unwrap();
		}
		MockEpochInfo::set_epoch(GENESIS_EPOCH);
		MockEpochInfo::set_epoch_authority_count(
			GENESIS_EPOCH,
			authorities.len() as AuthorityCount,
		);
		MockEpochInfo::set_authorities(authorities);
	});

	ext
}

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
	test_ext_inner(Some(GENESIS_AGG_PUB_KEY))
}

pub(crate) fn new_test_ext_no_key() -> sp_io::TestExternalities {
	test_ext_inner(None)
}
