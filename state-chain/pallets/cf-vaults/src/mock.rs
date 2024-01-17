#![cfg(test)]

use std::cell::RefCell;

use super::*;
use crate as pallet_cf_vaults;
use cf_chains::{
	btc,
	evm::SchnorrVerificationComponents,
	mocks::{MockAggKey, MockEthereum, MockEthereumChainCrypto},
	ApiCall, SetAggKeyWithAggKeyError,
};
use cf_primitives::{BroadcastId, FLIPPERINOS_PER_FLIP, GENESIS_EPOCH};
use cf_traits::{
	impl_mock_callback, impl_mock_chainflip, impl_mock_runtime_safe_mode,
	mocks::{
		block_height_provider::BlockHeightProvider, cfe_interface_mock::MockCfeInterface,
		threshold_signer::MockThresholdSigner,
	},
	AccountRoleRegistry,
};
use frame_support::{
	construct_runtime, parameter_types, traits::UnfilteredDispatchable, StorageHasher,
};
use sp_core::H256;
use sp_runtime::traits::{BlakeTwo256, IdentityLookup};

pub type ValidatorId = u64;

thread_local! {
	pub static BAD_VALIDATORS: RefCell<Vec<ValidatorId>> = RefCell::new(vec![]);
	pub static SET_AGG_KEY_WITH_AGG_KEY_REQUIRED: RefCell<bool> = RefCell::new(true);
	pub static SLASHES: RefCell<Vec<u64>> = RefCell::new(Default::default());
}

type Block = frame_system::mocking::MockBlock<Test>;

construct_runtime!(
	pub struct Test {
		System: frame_system,
		VaultsPallet: pallet_cf_vaults,
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
}

pub const ETH_DUMMY_SIG: SchnorrVerificationComponents =
	SchnorrVerificationComponents { s: [0xcf; 32], k_times_g_address: [0xcf; 20] };

pub const BTC_DUMMY_SIG: btc::Signature = [0xcf; 64];

impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Nonce = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
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

impl_mock_chainflip!(Test);
impl_mock_callback!(RuntimeOrigin);

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct MockSetAggKeyWithAggKey {
	old_key: <<MockEthereum as Chain>::ChainCrypto as ChainCrypto>::AggKey,
	new_key: <<MockEthereum as Chain>::ChainCrypto as ChainCrypto>::AggKey,
}

impl MockSetAggKeyWithAggKey {
	pub fn set_required(required: bool) {
		SET_AGG_KEY_WITH_AGG_KEY_REQUIRED.with(|cell| {
			*cell.borrow_mut() = required;
		});
	}
}

impl SetAggKeyWithAggKey<MockEthereumChainCrypto> for MockSetAggKeyWithAggKey {
	fn new_unsigned(
		old_key: Option<<<MockEthereum as Chain>::ChainCrypto as ChainCrypto>::AggKey>,
		new_key: <<MockEthereum as Chain>::ChainCrypto as ChainCrypto>::AggKey,
	) -> Result<Self, SetAggKeyWithAggKeyError> {
		if !SET_AGG_KEY_WITH_AGG_KEY_REQUIRED.with(|cell| *cell.borrow()) {
			return Err(SetAggKeyWithAggKeyError::NotRequired)
		}

		Ok(Self { old_key: old_key.ok_or(SetAggKeyWithAggKeyError::Failed)?, new_key })
	}
}

impl ApiCall<MockEthereumChainCrypto> for MockSetAggKeyWithAggKey {
	fn threshold_signature_payload(
		&self,
	) -> <<MockEthereum as Chain>::ChainCrypto as ChainCrypto>::Payload {
		unimplemented!()
	}

	fn signed(
		self,
		_threshold_signature: &<<MockEthereum as Chain>::ChainCrypto as ChainCrypto>::ThresholdSignature,
	) -> Self {
		unimplemented!()
	}

	fn chain_encoded(&self) -> Vec<u8> {
		unimplemented!()
	}

	fn is_signed(&self) -> bool {
		unimplemented!()
	}

	fn transaction_out_id(
		&self,
	) -> <<MockEthereum as Chain>::ChainCrypto as ChainCrypto>::TransactionOutId {
		todo!()
	}
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

	fn threshold_sign_and_broadcast(_api_call: Self::ApiCall) -> BroadcastId {
		Self::send_broadcast();
		1
	}

	fn threshold_sign_and_broadcast_with_callback(
		_api_call: Self::ApiCall,
		_success_callback: Option<Self::Callback>,
		_failed_callback_generator: impl FnOnce(BroadcastId) -> Option<Self::Callback>,
	) -> BroadcastId {
		unimplemented!()
	}

	fn threshold_sign_and_broadcast_rotation_tx(api_call: Self::ApiCall) -> BroadcastId {
		Self::threshold_sign_and_broadcast(api_call)
	}

	fn threshold_resign(_broadcast_id: BroadcastId) -> Option<ThresholdSignatureRequestId> {
		unimplemented!()
	}

	fn threshold_sign(_api_call: Self::ApiCall) -> (BroadcastId, ThresholdSignatureRequestId) {
		unimplemented!()
	}

	/// Clean up storage data related to a broadcast ID.
	fn clean_up_broadcast_storage(_broadcast_id: BroadcastId) {
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
	type Balance = u128;

	fn slash(validator_id: &Self::AccountId, _blocks: Self::BlockNumber) {
		// Count those slashes
		SLASHES.with(|count| {
			count.borrow_mut().push(*validator_id);
		});
	}

	fn slash_balance(account_id: &Self::AccountId, _amount: FlipBalance) {
		// Count those slashes
		SLASHES.with(|count| {
			count.borrow_mut().push(*account_id);
		});
	}

	fn calculate_slash_amount(
		_account_id: &Self::AccountId,
		_blocks: Self::BlockNumber,
	) -> Self::Balance {
		unimplemented!()
	}
}

impl_mock_runtime_safe_mode! { vault: PalletSafeMode<()> }

impl pallet_cf_vaults::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Offence = PalletOffence;
	type Chain = MockEthereum;
	type RuntimeCall = RuntimeCall;
	type EnsureThresholdSigned = NeverFailingOriginCheck<Self>;
	type ThresholdSigner = MockThresholdSigner<MockEthereumChainCrypto, RuntimeCall>;
	type OffenceReporter = MockOffenceReporter;
	type SetAggKeyWithAggKey = MockSetAggKeyWithAggKey;
	type WeightInfo = ();
	type Broadcaster = MockBroadcaster;
	type SafeMode = MockRuntimeSafeMode;
	type Slasher = MockSlasher;
	type ChainTracking = BlockHeightProvider<MockEthereum>;
	type CfeMultisigRequest = MockCfeInterface;
}

pub const ALICE: <Test as frame_system::Config>::AccountId = 123u64;
pub const BOB: <Test as frame_system::Config>::AccountId = 456u64;
pub const CHARLIE: <Test as frame_system::Config>::AccountId = 789u64;
pub const GENESIS_AGG_PUB_KEY: MockAggKey = MockAggKey(*b"genk");
pub const NEW_AGG_PUB_KEY_PRE_HANDOVER: MockAggKey = MockAggKey(*b"next");
pub const NEW_AGG_PUB_KEY_POST_HANDOVER: MockAggKey = MockAggKey(*b"hand");

pub const MOCK_KEYGEN_RESPONSE_TIMEOUT: u64 = 25;

cf_test_utilities::impl_test_helpers! {
	Test,
	RuntimeGenesisConfig {
		system: Default::default(),
		vaults_pallet: VaultsPalletConfig {
			vault_key: Some(GENESIS_AGG_PUB_KEY),
			deployment_block: 0,
			keygen_response_timeout: MOCK_KEYGEN_RESPONSE_TIMEOUT,
			amount_to_slash: FLIPPERINOS_PER_FLIP,
		},
	},
	|| {
		let authorities = BTreeSet::from([ALICE, BOB, CHARLIE]);
		for id in &authorities {
			<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_validator(id)
				.unwrap();
		}
		MockEpochInfo::set_epoch(GENESIS_EPOCH);
		MockEpochInfo::set_epoch_authority_count(
			GENESIS_EPOCH,
			authorities.len() as AuthorityCount,
		);
		MockEpochInfo::set_authorities(authorities);
	},
}

pub(crate) fn new_test_ext_no_key() -> TestRunner<()> {
	TestRunner::<()>::new(RuntimeGenesisConfig::default())
}
