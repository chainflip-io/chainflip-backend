pub use crate::{self as pallet_cf_ingress_egress};
use crate::{DepositWitness, PalletSafeMode, WhitelistedBrokers};

pub use cf_chains::{
	address::{AddressDerivationApi, AddressDerivationError, ForeignChainAddress},
	eth::EthereumTrackedData,
	evm::U256,
};
use cf_chains::{Chain, ChannelRefundParameters};
use cf_primitives::ChannelId;
pub use cf_primitives::{
	chains::{assets, Ethereum},
	Asset,
};
use cf_test_utilities::{impl_test_helpers, TestExternalities};
use cf_traits::{
	impl_mock_chainflip, impl_mock_runtime_safe_mode,
	mocks::{
		address_converter::MockAddressConverter,
		affiliate_registry::MockAffiliateRegistry,
		api_call::{MockEthereumApiCall, MockEvmEnvironment},
		asset_converter::MockAssetConverter,
		asset_withholding::MockAssetWithholding,
		balance_api::MockBalance,
		broadcaster::MockBroadcaster,
		chain_tracking::ChainTracker,
		fee_payment::MockFeePayment,
		fetches_transfers_limit_provider::MockFetchesTransfersLimitProvider,
		swap_limits_provider::MockSwapLimitsProvider,
		swap_request_api::MockSwapRequestHandler,
	},
	AccountRoleRegistry, DepositApi, DummyIngressSource, NetworkEnvironmentProvider, OnDeposit,
};
use frame_support::{assert_ok, derive_impl};
use frame_system as system;
use sp_core::{ConstBool, ConstU64};
use sp_runtime::traits::Zero;

type AccountId = u64;
type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		IngressEgress: pallet_cf_ingress_egress,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl system::Config for Test {
	type Block = Block;
}

impl_mock_chainflip!(Test);

pub struct MockDepositHandler;
impl OnDeposit<Ethereum> for MockDepositHandler {}

pub type MockEgressBroadcaster =
	MockBroadcaster<(MockEthereumApiCall<MockEvmEnvironment>, RuntimeCall)>;

pub struct MockAddressDerivation;

impl AddressDerivationApi<Ethereum> for MockAddressDerivation {
	fn generate_address(
		_source_asset: assets::eth::Asset,
		channel_id: ChannelId,
	) -> Result<<Ethereum as Chain>::ChainAccount, AddressDerivationError> {
		Ok([channel_id as u8; 20].into())
	}

	fn generate_address_and_state(
		source_asset: <Ethereum as Chain>::ChainAsset,
		channel_id: ChannelId,
	) -> Result<
		(<Ethereum as Chain>::ChainAccount, <Ethereum as Chain>::DepositChannelState),
		AddressDerivationError,
	> {
		Ok((Self::generate_address(source_asset, channel_id)?, Default::default()))
	}
}

pub struct MockNetworkEnvironmentProvider {}

impl NetworkEnvironmentProvider for MockNetworkEnvironmentProvider {
	fn get_network_environment() -> cf_primitives::NetworkEnvironment {
		cf_primitives::NetworkEnvironment::Development
	}
}

impl_mock_runtime_safe_mode! { ingress_egress_ethereum: PalletSafeMode<()> }

impl crate::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	const MANAGE_CHANNEL_LIFETIME: bool = true;
	type IngressSource = DummyIngressSource<Ethereum>;
	type TargetChain = Ethereum;
	type AddressDerivation = MockAddressDerivation;
	type AddressConverter = MockAddressConverter;
	type Balance = MockBalance;
	type PoolApi = Self;
	type ChainApiCall = MockEthereumApiCall<MockEvmEnvironment>;
	type Broadcaster = MockEgressBroadcaster;
	type DepositHandler = MockDepositHandler;
	type ChainTracking = ChainTracker<Ethereum>;
	type WeightInfo = ();
	type NetworkEnvironment = MockNetworkEnvironmentProvider;
	type AssetConverter = MockAssetConverter;
	type FeePayment = MockFeePayment<Self>;
	type SwapRequestHandler =
		MockSwapRequestHandler<(Ethereum, pallet_cf_ingress_egress::Pallet<Self>)>;
	type AssetWithholding = MockAssetWithholding;
	type FetchesTransfersLimitProvider = MockFetchesTransfersLimitProvider;
	type SafeMode = MockRuntimeSafeMode;
	type SwapLimitsProvider = MockSwapLimitsProvider;
	type CcmValidityChecker = cf_chains::ccm_checker::CcmValidityChecker;
	type AffiliateRegistry = MockAffiliateRegistry;
	type AllowTransactionReports = ConstBool<true>;
	type ScreeningBrokerId = ConstU64<SCREENING_ID>;
}

pub const ALICE: <Test as frame_system::Config>::AccountId = 123u64;
pub const BROKER: <Test as frame_system::Config>::AccountId = 456u64;
pub const WHITELISTED_BROKER: <Test as frame_system::Config>::AccountId = BROKER + 1;
pub const SCREENING_ID: <Test as frame_system::Config>::AccountId = 0xcf;

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
		cf_traits::mocks::tracked_data_provider::TrackedDataProvider::<Ethereum>::set_tracked_data(
			EthereumTrackedData {
				base_fee: Default::default(),
				priority_fee: Default::default()
			}
		);
		assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_broker(&BROKER));
		assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_broker(
			&WHITELISTED_BROKER,
		));
		WhitelistedBrokers::<Test>::insert(WHITELISTED_BROKER, ());
	}
}

type TestChainAccount = <<Test as crate::Config>::TargetChain as Chain>::ChainAccount;
type TestChainAmount = <<Test as crate::Config>::TargetChain as Chain>::ChainAmount;
type TestChainAsset = <<Test as crate::Config>::TargetChain as Chain>::ChainAsset;

pub trait RequestAddressAndDeposit {
	fn request_address_and_deposit(
		self,
		requests: &[(DepositRequest, TestChainAmount)],
	) -> TestExternalities<Test, Vec<(DepositRequest, ChannelId, TestChainAccount)>>;
}

impl<Ctx: Clone> RequestAddressAndDeposit for TestRunner<Ctx> {
	/// Request deposit addresses and complete the deposit of funds into those addresses.
	#[track_caller]
	fn request_address_and_deposit(
		self,
		requests: &[(DepositRequest, TestChainAmount)],
	) -> TestExternalities<Test, Vec<(DepositRequest, ChannelId, TestChainAccount)>> {
		let (requests, amounts): (Vec<_>, Vec<_>) = requests.iter().cloned().unzip();

		self.request_deposit_addresses(&requests[..])
			.then_execute_at_next_block(move |channels| {
				channels
					.into_iter()
					.zip(amounts)
					.map(|((request, channel_id, deposit_address), amount)| {
						if !amount.is_zero() {
							IngressEgress::process_channel_deposit_full_witness_inner(
								&DepositWitness {
									deposit_address,
									asset: request.source_asset(),
									amount,
									deposit_details: Default::default(),
								},
								Default::default(),
							)
							.unwrap()
						}
						(request, channel_id, deposit_address)
					})
					.collect::<Vec<_>>()
			})
	}
}

#[derive(Clone, Debug)]
pub enum DepositRequest {
	Liquidity {
		lp_account: AccountId,
		asset: TestChainAsset,
	},
	/// Do a non-ccm swap using a default broker and no fees.
	SimpleSwap {
		source_asset: TestChainAsset,
		destination_asset: TestChainAsset,
		destination_address: ForeignChainAddress,
		refund_address: TestChainAccount,
	},
}

impl DepositRequest {
	pub fn source_asset(&self) -> TestChainAsset {
		match self {
			Self::Liquidity { asset, .. } => *asset,
			Self::SimpleSwap { source_asset, .. } => *source_asset,
		}
	}
}

pub trait RequestAddress {
	fn request_deposit_addresses(
		self,
		requests: &[DepositRequest],
	) -> TestExternalities<Test, Vec<(DepositRequest, ChannelId, TestChainAccount)>>;
}

impl<Ctx: Clone> RequestAddress for TestExternalities<Test, Ctx> {
	#[track_caller]
	fn request_deposit_addresses(
		self,
		requests: &[DepositRequest],
	) -> TestExternalities<Test, Vec<(DepositRequest, ChannelId, TestChainAccount)>> {
		self.then_execute_at_next_block(|_| {
			requests
				.iter()
				.cloned()
				.map(|request| match request {
					DepositRequest::Liquidity { lp_account, asset } =>
						IngressEgress::request_liquidity_deposit_address(
							lp_account,
							asset,
							0,
							ForeignChainAddress::Eth(Default::default()),
						)
						.map(|(id, addr, ..)| {
							(request, id, TestChainAccount::try_from(addr).unwrap())
						})
						.unwrap(),
					DepositRequest::SimpleSwap {
						source_asset,
						destination_asset,
						ref destination_address,
						refund_address,
					} => IngressEgress::request_swap_deposit_address(
						source_asset,
						destination_asset.into(),
						destination_address.clone(),
						Default::default(),
						BROKER,
						None,
						10,
						ChannelRefundParameters {
							retry_duration: 5,
							refund_address: ForeignChainAddress::Eth(refund_address),
							min_price: U256::zero(),
						},
						None,
					)
					.map(|(channel_id, deposit_address, ..)| {
						(request, channel_id, TestChainAccount::try_from(deposit_address).unwrap())
					})
					.unwrap(),
				})
				.collect()
		})
	}
}
