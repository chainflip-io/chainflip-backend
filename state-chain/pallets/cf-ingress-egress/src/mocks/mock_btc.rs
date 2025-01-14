pub use crate::{self as pallet_cf_ingress_egress};

use crate::PalletSafeMode;
use cf_chains::btc::{deposit_address::DepositAddress, BitcoinTrackedData};
pub use cf_chains::{
	address::{AddressDerivationApi, AddressDerivationError},
	Chain,
};
pub use cf_primitives::chains::{assets, Bitcoin};
use cf_primitives::ChannelId;
use cf_test_utilities::impl_test_helpers;
use cf_traits::{
	impl_mock_chainflip, impl_mock_runtime_safe_mode,
	mocks::{
		address_converter::MockAddressConverter,
		affiliate_registry::MockAffiliateRegistry,
		api_call::{MockBitcoinApiCall, MockBtcEnvironment},
		asset_converter::MockAssetConverter,
		asset_withholding::MockAssetWithholding,
		balance_api::MockBalance,
		broadcaster::MockBroadcaster,
		chain_tracking::ChainTracker,
		fee_payment::MockFeePayment,
		swap_limits_provider::MockSwapLimitsProvider,
		swap_request_api::MockSwapRequestHandler,
	},
	DummyIngressSource, NetworkEnvironmentProvider, OnDeposit,
};
use frame_support::derive_impl;
use sp_core::ConstBool;

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		IngressEgress: pallet_cf_ingress_egress,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

impl_mock_chainflip!(Test);

pub struct MockDepositHandler;
impl OnDeposit<Bitcoin> for MockDepositHandler {}

pub type MockEgressBroadcaster =
	MockBroadcaster<(MockBitcoinApiCall<MockBtcEnvironment>, RuntimeCall)>;

pub struct MockAddressDerivation;

impl AddressDerivationApi<Bitcoin> for MockAddressDerivation {
	fn generate_address(
		_source_asset: assets::btc::Asset,
		_channel_id: ChannelId,
	) -> Result<<Bitcoin as Chain>::ChainAccount, AddressDerivationError> {
		Ok(cf_chains::btc::ScriptPubkey::Taproot([0u8; 32]))
	}

	fn generate_address_and_state(
		source_asset: <Bitcoin as Chain>::ChainAsset,
		channel_id: ChannelId,
	) -> Result<
		(<Bitcoin as Chain>::ChainAccount, <Bitcoin as Chain>::DepositChannelState),
		AddressDerivationError,
	> {
		Ok((Self::generate_address(source_asset, channel_id)?, DepositAddress::new([1u8; 32], 123)))
	}
}

pub struct MockNetworkEnvironmentProvider {}

impl NetworkEnvironmentProvider for MockNetworkEnvironmentProvider {
	fn get_network_environment() -> cf_primitives::NetworkEnvironment {
		cf_primitives::NetworkEnvironment::Development
	}
}

impl_mock_runtime_safe_mode! { ingress_egress_bitcoin: PalletSafeMode<()> }

impl pallet_cf_ingress_egress::Config for Test {
	const NAME: &'static str = "Mock";
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	const MANAGE_CHANNEL_LIFETIME: bool = true;
	type IngressSource = DummyIngressSource<Bitcoin>;
	type TargetChain = Bitcoin;
	type AddressDerivation = MockAddressDerivation;
	type AddressConverter = MockAddressConverter;
	type Balance = MockBalance;
	type PoolApi = Self;
	type ChainApiCall = MockBitcoinApiCall<MockBtcEnvironment>;
	type Broadcaster = MockEgressBroadcaster;
	type DepositHandler = MockDepositHandler;
	type ChainTracking = ChainTracker<Bitcoin>;
	type WeightInfo = ();
	type NetworkEnvironment = MockNetworkEnvironmentProvider;
	type AssetConverter = MockAssetConverter;
	type FeePayment = MockFeePayment<Self>;
	type SwapRequestHandler =
		MockSwapRequestHandler<(Bitcoin, pallet_cf_ingress_egress::Pallet<Self>)>;
	type AssetWithholding = MockAssetWithholding;
	type FetchesTransfersLimitProvider = cf_traits::NoLimit;
	type SafeMode = MockRuntimeSafeMode;
	type SwapLimitsProvider = MockSwapLimitsProvider;
	type CcmValidityChecker = cf_chains::ccm_checker::CcmValidityChecker;
	type AllowTransactionReports = ConstBool<true>;
	type AffiliateRegistry = MockAffiliateRegistry;
}

impl_test_helpers! {
	Test,
	RuntimeGenesisConfig {
		system: Default::default(),
		ingress_egress: IngressEgressConfig {
			deposit_channel_lifetime: 100,
			witness_safety_margin: Some(2),
			dust_limits: Default::default(),
		},
	},
	|| {
		cf_traits::mocks::tracked_data_provider::TrackedDataProvider::<Bitcoin>::set_tracked_data(
			BitcoinTrackedData { btc_fee_info: Default::default() }
		);
	}
}
