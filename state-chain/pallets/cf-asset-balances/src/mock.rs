use crate as pallet_cf_asset_balances;
use crate::PalletSafeMode;
use cf_chains::{
	btc::ScriptPubkey,
	dot::{PolkadotAccountId, PolkadotCrypto},
	AnyChain,
};
use cf_primitives::AccountId;

use cf_traits::{
	impl_mock_chainflip, impl_mock_runtime_safe_mode,
	mocks::{egress_handler::MockEgressHandler, key_provider::MockKeyProvider},
};
use frame_support::{derive_impl, sp_runtime::app_crypto::sp_core::H160};
use frame_system as system;
use sp_runtime::traits::IdentityLookup;

use cf_chains::ForeignChainAddress;

type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		AssetBalances: pallet_cf_asset_balances,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl system::Config for Test {
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
}

impl_mock_chainflip!(Test);

pub const ETH_ADDR_1: ForeignChainAddress = ForeignChainAddress::Eth(H160([0; 20]));
pub const ETH_ADDR_2: ForeignChainAddress = ForeignChainAddress::Eth(H160([1; 20]));
pub const ARB_ADDR_1: ForeignChainAddress = ForeignChainAddress::Arb(H160([2; 20]));

pub const DOT_ADDR_1: ForeignChainAddress =
	ForeignChainAddress::Dot(PolkadotAccountId::from_aliased([1; 32]));

pub const BTC_ADDR_1: ForeignChainAddress =
	ForeignChainAddress::Btc(ScriptPubkey::Taproot([1u8; 32]));

pub const SOL_ADDR: ForeignChainAddress =
	ForeignChainAddress::Sol(cf_chains::sol::SolAddress([1u8; 32]));

impl_mock_runtime_safe_mode!(refunding: PalletSafeMode);

impl crate::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type EgressHandler = MockEgressHandler<AnyChain>;
	type PolkadotKeyProvider = MockKeyProvider<PolkadotCrypto>;
	type SafeMode = MockRuntimeSafeMode;
}

cf_test_utilities::impl_test_helpers! {
	Test,
	RuntimeGenesisConfig::default(),
	|| {
		MockKeyProvider::<PolkadotCrypto>::set_key(
			PolkadotAccountId::from_aliased([0xff; 32]),
		);
	}
}
