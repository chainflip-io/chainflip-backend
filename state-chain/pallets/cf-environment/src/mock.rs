#![cfg(test)]

use crate::{self as pallet_cf_environment, cfe, Decode, Encode, TypeInfo};
use cf_chains::{
	btc::{BitcoinFeeInfo, BitcoinNetwork},
	dot::{api::CreatePolkadotVault, TEST_RUNTIME_VERSION},
	ApiCall, Bitcoin, Chain, ChainCrypto, Polkadot,
};
use cf_primitives::{
	BroadcastId, ThresholdSignatureRequestId, INPUT_UTXO_SIZE_IN_BYTES,
	MINIMUM_BTC_TX_SIZE_IN_BYTES, OUTPUT_UTXO_SIZE_IN_BYTES,
};
use cf_traits::{
	impl_mock_callback, impl_mock_chainflip, impl_mock_runtime_safe_mode, impl_pallet_safe_mode,
	BroadcastCleanup, Broadcaster, GetBitcoinFeeInfo, VaultKeyWitnessedHandler,
};
use frame_support::{parameter_types, traits::UnfilteredDispatchable};
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};

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

impl_mock_chainflip!(Test);

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

	fn transaction_out_id(&self) -> <Polkadot as ChainCrypto>::TransactionOutId {
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
		_block_number: <Polkadot as Chain>::ChainBlockNumber,
	) -> frame_support::pallet_prelude::DispatchResultWithPostInfo {
		unimplemented!()
	}
}
pub struct MockBitcoinVaultKeyWitnessedHandler;
impl VaultKeyWitnessedHandler<Bitcoin> for MockBitcoinVaultKeyWitnessedHandler {
	fn on_new_key_activated(
		_block_number: <Bitcoin as Chain>::ChainBlockNumber,
	) -> frame_support::pallet_prelude::DispatchResultWithPostInfo {
		unimplemented!()
	}
}

parameter_types! {
	pub const BitcoinNetworkParam: BitcoinNetwork = BitcoinNetwork::Testnet;
}

pub struct MockBitcoinFeeInfo;
impl GetBitcoinFeeInfo for MockBitcoinFeeInfo {
	fn bitcoin_fee_info() -> BitcoinFeeInfo {
		BitcoinFeeInfo {
			fee_per_input_utxo: 10 * INPUT_UTXO_SIZE_IN_BYTES,
			fee_per_output_utxo: 10 * OUTPUT_UTXO_SIZE_IN_BYTES,
			min_fee_required_per_tx: 10 * MINIMUM_BTC_TX_SIZE_IN_BYTES,
		}
	}
}

impl_pallet_safe_mode!(MockPalletSafeMode; flag1, flag2);
impl_mock_runtime_safe_mode!(mock: MockPalletSafeMode);

impl pallet_cf_environment::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type CreatePolkadotVault = MockCreatePolkadotVault;
	type PolkadotBroadcaster = MockPolkadotBroadcaster;
	type BitcoinNetwork = BitcoinNetworkParam;
	type PolkadotVaultKeyWitnessedHandler = MockPolkadotVaultKeyWitnessedHandler;
	type BitcoinVaultKeyWitnessedHandler = MockBitcoinVaultKeyWitnessedHandler;
	type BitcoinFeeInfo = MockBitcoinFeeInfo;
	type RuntimeSafeMode = MockRuntimeSafeMode;
	type WeightInfo = ();
}

pub const STATE_CHAIN_GATEWAY_ADDRESS: [u8; 20] = [0u8; 20];
pub const ETH_KEY_MANAGER_ADDRESS: [u8; 20] = [1u8; 20];
pub const ETH_VAULT_ADDRESS: [u8; 20] = [2u8; 20];
pub const ARB_KEY_MANAGER_ADDRESS: [u8; 20] = [3u8; 20];
pub const ARB_VAULT_ADDRESS: [u8; 20] = [4u8; 20];
pub const ARBETH_TOKEN_ADDRESS: [u8; 20] = [5u8; 20];
pub const ADDRESS_CHECKER: [u8; 20] = [3u8; 20];
pub const ETH_CHAIN_ID: u64 = 1;
pub const ARB_CHAIN_ID: u64 = 2;

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
			state_chain_gateway_address: STATE_CHAIN_GATEWAY_ADDRESS,
			key_manager_address: ETH_KEY_MANAGER_ADDRESS,
			ethereum_chain_id: ETH_CHAIN_ID,
			arbitrum_chain_id: ARB_CHAIN_ID,
			eth_vault_address: ETH_VAULT_ADDRESS,
			arb_key_manager_address: ARB_KEY_MANAGER_ADDRESS,
			arb_vault_address: ARB_VAULT_ADDRESS,
			arbeth_token_address: ARBETH_TOKEN_ADDRESS,
			eth_address_checker_address: ADDRESS_CHECKER,
			cfe_settings: CFE_SETTINGS,
			flip_token_address: [0u8; 20],
			eth_usdc_address: [0x2; 20],
			polkadot_genesis_hash: H256([0u8; 32]),
			polkadot_vault_account_id: None,
			polkadot_runtime_version: TEST_RUNTIME_VERSION,
			bitcoin_network: Default::default(),
		},
	};

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
