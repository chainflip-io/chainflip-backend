use crate::{self as pallet_cf_environment, cfe};
#[cfg(feature = "ibiza")]
use cf_chains::dot::POLKADOT_METADATA;
#[cfg(feature = "ibiza")]
use cf_chains::{dot::api::CreatePolkadotVault, ApiCall, Chain, ChainCrypto, Polkadot};

#[cfg(feature = "ibiza")]
use cf_primitives::BroadcastId;
use cf_traits::mocks::ensure_origin_mock::NeverFailingOriginCheck;
#[cfg(feature = "ibiza")]
use cf_traits::{Broadcaster, VaultKeyWitnessedHandler};

use frame_support::parameter_types;
use frame_system as system;
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};

#[cfg(feature = "ibiza")]
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

impl system::Config for Test {
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

#[cfg(feature = "ibiza")]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct MockCreatePolkadotVault {
	agg_key: cf_chains::dot::PolkadotPublicKey,
}
#[cfg(feature = "ibiza")]
impl CreatePolkadotVault for MockCreatePolkadotVault {
	fn new_unsigned(proxy_key: cf_chains::dot::PolkadotPublicKey) -> Self {
		Self { agg_key: proxy_key }
	}
}
#[cfg(feature = "ibiza")]
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
#[cfg(feature = "ibiza")]
pub struct MockPolkadotBroadcaster;
#[cfg(feature = "ibiza")]
impl Broadcaster<Polkadot> for MockPolkadotBroadcaster {
	type ApiCall = MockCreatePolkadotVault;

	fn threshold_sign_and_broadcast(_api_call: Self::ApiCall) -> BroadcastId {
		unimplemented!()
	}
}
#[cfg(feature = "ibiza")]
pub struct MockPolkadotVaultKeyWitnessedHandler;
#[cfg(feature = "ibiza")]
impl VaultKeyWitnessedHandler<Polkadot> for MockPolkadotVaultKeyWitnessedHandler {
	fn on_new_key_activated(
		_new_public_key: <Polkadot as ChainCrypto>::AggKey,
		_block_number: <Polkadot as Chain>::ChainBlockNumber,
		_tx_id: <Polkadot as ChainCrypto>::TransactionId,
	) -> frame_support::pallet_prelude::DispatchResultWithPostInfo {
		unimplemented!()
	}
}

impl pallet_cf_environment::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type EnsureGovernance = NeverFailingOriginCheck<Self>;
	#[cfg(feature = "ibiza")]
	type CreatePolkadotVault = MockCreatePolkadotVault;
	#[cfg(feature = "ibiza")]
	type PolkadotBroadcaster = MockPolkadotBroadcaster;
	#[cfg(feature = "ibiza")]
	type PolkadotVaultKeyWitnessedHandler = MockPolkadotVaultKeyWitnessedHandler;
	type WeightInfo = ();
}

pub const STAKE_MANAGER_ADDRESS: [u8; 20] = [0u8; 20];
pub const KEY_MANAGER_ADDRESS: [u8; 20] = [1u8; 20];
pub const VAULT_ADDRESS: [u8; 20] = [2u8; 20];
pub const ETH_CHAIN_ID: u64 = 1;

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
			#[cfg(feature = "ibiza")]
			polkadot_vault_account_id: None,
			#[cfg(feature = "ibiza")]
			polkadot_network_metadata: POLKADOT_METADATA,
		},
	};

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
