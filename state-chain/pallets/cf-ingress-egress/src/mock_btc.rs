pub use crate::{self as pallet_cf_ingress_egress, Instance3};

use cf_chains::btc::{deposit_address::DepositAddress, BitcoinTrackedData};
pub use cf_chains::{
	address::{AddressDerivationApi, AddressDerivationError, ForeignChainAddress},
	Chain,
};
pub use cf_primitives::{
	chains::{assets, Bitcoin},
	Asset, AssetAmount,
};
use cf_primitives::{ChannelId, SwapId};
use cf_test_utilities::impl_test_helpers;
use cf_traits::{
	impl_mock_callback, impl_mock_chainflip,
	mocks::{
		address_converter::MockAddressConverter,
		api_call::{MockBitcoinApiCall, MockBtcEnvironment},
		asset_converter::MockAssetConverter,
		broadcaster::MockBroadcaster,
		ccm_handler::MockCcmHandler,
		fee_payment::MockFeePayment,
		lp_balance::MockBalance,
	},
	DepositHandler, NetworkEnvironmentProvider, SwapDepositHandler,
};
use frame_support::{derive_impl, traits::UnfilteredDispatchable};
use sp_core::H256;
use sp_runtime::traits::{BlakeTwo256, IdentityLookup};

type AccountId = u64;
type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		IngressEgress: pallet_cf_ingress_egress::<Instance3>,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Nonce = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = frame_support::traits::ConstU64<250>;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = frame_support::traits::ConstU16<2112>;
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<5>;
}

impl_mock_chainflip!(Test);
impl_mock_callback!(RuntimeOrigin);

pub struct MockDepositHandler;
impl DepositHandler<Bitcoin> for MockDepositHandler {}

pub struct MockSwapDepositHandlerBtc;
impl SwapDepositHandler for MockSwapDepositHandlerBtc {
	type AccountId = AccountId;

	fn schedule_swap_from_channel(
		_deposit_address: ForeignChainAddress,
		_deposit_block_height: u64,
		_from: Asset,
		_to: Asset,
		_amount: AssetAmount,
		_destination_address: ForeignChainAddress,
		_broker_id: Self::AccountId,
		_broker_commission_bps: cf_primitives::BasisPoints,
		_channel_id: ChannelId,
	) -> SwapId {
		unimplemented!()
	}
}

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

impl pallet_cf_ingress_egress::Config<Instance3> for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type TargetChain = Bitcoin;
	type AddressDerivation = MockAddressDerivation;
	type AddressConverter = MockAddressConverter;
	type LpBalance = MockBalance;
	type SwapDepositHandler = MockSwapDepositHandlerBtc;
	type ChainApiCall = MockBitcoinApiCall<MockBtcEnvironment>;
	type Broadcaster = MockEgressBroadcaster;
	type DepositHandler = MockDepositHandler;
	type CcmHandler = MockCcmHandler;
	type ChainTracking = cf_traits::mocks::chain_tracking::ChainTracking<Bitcoin>;
	type WeightInfo = ();
	type NetworkEnvironment = MockNetworkEnvironmentProvider;
	type AssetConverter = MockAssetConverter;
	type FeePayment = MockFeePayment<Self>;
}

impl_test_helpers! {
	Test,
	RuntimeGenesisConfig {
		system: Default::default(),
		ingress_egress: IngressEgressConfig { deposit_channel_lifetime: 100, witness_safety_margin: Some(2), dust_limits: Default::default() },
	},
	|| {
		cf_traits::mocks::tracked_data_provider::TrackedDataProvider::<Bitcoin>::set_tracked_data(
			BitcoinTrackedData { btc_fee_info: Default::default() }
		);
	}
}
