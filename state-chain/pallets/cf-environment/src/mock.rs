use crate::{self as pallet_cf_environment, cfe};
use cf_chains::{
	btc::BitcoinNetwork,
	dot::{api::CreatePolkadotVault, TEST_RUNTIME_VERSION},
	ApiCall, Bitcoin, Chain, ChainCrypto, Polkadot,
};

use cf_primitives::{AuthorityCount, BroadcastId, ThresholdSignatureRequestId};
use cf_traits::{
	impl_mock_callback,
	mocks::{ensure_origin_mock::NeverFailingOriginCheck, system_state_info::MockSystemStateInfo},
	BroadcastCleanup, Broadcaster, Chainflip, VaultKeyWitnessedHandler,
};

use frame_support::{parameter_types, traits::UnfilteredDispatchable};
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};

use crate::{Decode, Encode, TypeInfo};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;
type AccountId = u64;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system,
		Environment: pallet_cf_environment,
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
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Index = u64;
	type BlockNumber = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = Header;
	type RuntimeEvent = RuntimeEvent;
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

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct MockCreatePolkadotVault;

impl CreatePolkadotVault for MockCreatePolkadotVault {
	fn new_unsigned() -> Self {
		Self
	}
}
impl ApiCall<Polkadot> for MockCreatePolkadotVault {
	fn threshold_signature_payload(&self) -> <Polkadot as cf_chains::ChainCrypto>::Payload {
		unimplemented!()
	}
	fn chain_encoded(&self) -> Vec<u8> {
		unimplemented!()
	}
	fn signed(
		self,
		_threshold_signature: &<Polkadot as cf_chains::ChainCrypto>::ThresholdSignature,
	) -> Self {
		unimplemented!()
	}
	fn is_signed(&self) -> bool {
		unimplemented!()
	}
}

impl_mock_callback!(RuntimeOrigin);

pub struct MockPolkadotBroadcaster;
impl Broadcaster<Polkadot> for MockPolkadotBroadcaster {
	type ApiCall = MockCreatePolkadotVault;
	type Callback = MockCallback;

	fn threshold_sign_and_broadcast(
		_api_call: Self::ApiCall,
	) -> (BroadcastId, ThresholdSignatureRequestId) {
		unimplemented!()
	}

	fn threshold_sign_and_broadcast_with_callback(
		_api_call: Self::ApiCall,
		_callback: Self::Callback,
	) -> (BroadcastId, ThresholdSignatureRequestId) {
		unimplemented!()
	}
}
impl BroadcastCleanup<Polkadot> for MockPolkadotBroadcaster {
	fn clean_up_broadcast(_broadcast_id: BroadcastId) -> sp_runtime::DispatchResult {
		unimplemented!()
	}
}

pub struct MockPolkadotVaultKeyWitnessedHandler;
impl VaultKeyWitnessedHandler<Polkadot> for MockPolkadotVaultKeyWitnessedHandler {
	fn on_new_key_activated(
		_new_public_key: <Polkadot as ChainCrypto>::AggKey,
		_block_number: <Polkadot as Chain>::ChainBlockNumber,
		_tx_id: <Polkadot as ChainCrypto>::TransactionId,
	) -> frame_support::pallet_prelude::DispatchResultWithPostInfo {
		unimplemented!()
	}
}
pub struct MockBitcoinVaultKeyWitnessedHandler;
impl VaultKeyWitnessedHandler<Bitcoin> for MockBitcoinVaultKeyWitnessedHandler {
	fn on_new_key_activated(
		_new_public_key: <Bitcoin as ChainCrypto>::AggKey,
		_block_number: <Bitcoin as Chain>::ChainBlockNumber,
		_tx_id: <Bitcoin as ChainCrypto>::TransactionId,
	) -> frame_support::pallet_prelude::DispatchResultWithPostInfo {
		unimplemented!()
	}
}

cf_traits::impl_mock_ensure_witnessed_for_origin!(RuntimeOrigin);
cf_traits::impl_mock_epoch_info!(AccountId, u128, u32, AuthorityCount);

impl Chainflip for Test {
	type ValidatorId = AccountId;
	type Amount = u128;
	type RuntimeCall = RuntimeCall;
	type EnsureWitnessed = MockEnsureWitnessed;
	type EnsureWitnessedAtCurrentEpoch = MockEnsureWitnessed;
	type EpochInfo = MockEpochInfo;
	type SystemState = MockSystemStateInfo;
}

parameter_types! {
	pub const BitcoinNetworkParam: BitcoinNetwork = BitcoinNetwork::Testnet;
}

impl pallet_cf_environment::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type EnsureGovernance = NeverFailingOriginCheck<Self>;
	type CreatePolkadotVault = MockCreatePolkadotVault;
	type PolkadotBroadcaster = MockPolkadotBroadcaster;
	type BitcoinNetwork = BitcoinNetworkParam;
	type PolkadotVaultKeyWitnessedHandler = MockPolkadotVaultKeyWitnessedHandler;
	type BitcoinVaultKeyWitnessedHandler = MockBitcoinVaultKeyWitnessedHandler;
	type WeightInfo = ();
}

pub const STAKE_MANAGER_ADDRESS: [u8; 20] = [0u8; 20];
pub const KEY_MANAGER_ADDRESS: [u8; 20] = [1u8; 20];
pub const VAULT_ADDRESS: [u8; 20] = [2u8; 20];
pub const ETH_CHAIN_ID: u64 = 1;
pub const MOCK_FEE_PER_UTXO: u64 = 10;

pub const CFE_SETTINGS: cfe::CfeSettings = cfe::CfeSettings {
	eth_block_safety_margin: 1,
	max_ceremony_stage_duration: 1,
	eth_priority_fee_percentile: 50,
};

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let config = GenesisConfig {
		system: Default::default(),
		environment: EnvironmentConfig {
			stake_manager_address: STAKE_MANAGER_ADDRESS,
			key_manager_address: KEY_MANAGER_ADDRESS,
			ethereum_chain_id: ETH_CHAIN_ID,
			eth_vault_address: VAULT_ADDRESS,
			cfe_settings: CFE_SETTINGS,
			flip_token_address: [0u8; 20],
			eth_usdc_address: [0x2; 20],
			polkadot_genesis_hash: H256([0u8; 32]),
			polkadot_vault_account_id: None,
			polkadot_runtime_version: TEST_RUNTIME_VERSION,
			bitcoin_network: Default::default(),
			bitcoin_fee_per_utxo: MOCK_FEE_PER_UTXO,
		},
	};

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
