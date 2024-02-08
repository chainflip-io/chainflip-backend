pub use crate::{self as pallet_cf_ingress_egress};
use crate::{DepositBalances, DepositWitness};

use cf_chains::eth::EthereumTrackedData;
pub use cf_chains::{
	address::{AddressDerivationApi, AddressDerivationError, ForeignChainAddress},
	eth::Address as EthereumAddress,
	CcmDepositMetadata, Chain,
};
use cf_primitives::ChannelId;
pub use cf_primitives::{
	chains::{assets, Ethereum},
	Asset,
};
use cf_test_utilities::{impl_test_helpers, TestExternalities};
use cf_traits::{
	impl_mock_callback, impl_mock_chainflip,
	mocks::{
		address_converter::MockAddressConverter,
		api_call::{MockEthEnvironment, MockEthereumApiCall},
		asset_converter::MockAssetConverter,
		broadcaster::MockBroadcaster,
		ccm_handler::MockCcmHandler,
		fee_payment::MockFeePayment,
		lp_balance::MockBalance,
		swap_deposit_handler::MockSwapDepositHandler,
	},
	DepositApi, DepositHandler, NetworkEnvironmentProvider,
};
use frame_support::{
	derive_impl,
	traits::{OriginTrait, UnfilteredDispatchable},
};
use frame_system as system;
use sp_core::H256;
use sp_runtime::traits::{BlakeTwo256, IdentityLookup, Zero};

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
impl DepositHandler<Ethereum> for MockDepositHandler {}

pub type MockEgressBroadcaster =
	MockBroadcaster<(MockEthereumApiCall<MockEthEnvironment>, RuntimeCall)>;

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

impl crate::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type TargetChain = Ethereum;
	type AddressDerivation = MockAddressDerivation;
	type AddressConverter = MockAddressConverter;
	type LpBalance = MockBalance;
	type SwapDepositHandler =
		MockSwapDepositHandler<(Ethereum, pallet_cf_ingress_egress::Pallet<Self>)>;
	type ChainApiCall = MockEthereumApiCall<MockEthEnvironment>;
	type Broadcaster = MockEgressBroadcaster;
	type DepositHandler = MockDepositHandler;
	type CcmHandler = MockCcmHandler;
	type ChainTracking = cf_traits::mocks::chain_tracking::ChainTracking<Ethereum>;
	type WeightInfo = ();
	type NetworkEnvironment = MockNetworkEnvironmentProvider;
	type AssetConverter = MockAssetConverter;
	type FeePayment = MockFeePayment<Self>;
}

pub const ALICE: <Test as frame_system::Config>::AccountId = 123u64;
pub const BROKER: <Test as frame_system::Config>::AccountId = 456u64;

impl_test_helpers! {
	Test,
	RuntimeGenesisConfig {
		system: Default::default(),
		ingress_egress: IngressEgressConfig { deposit_channel_lifetime: 100, witness_safety_margin: Some(2), dust_limits: Default::default() },
	},
	|| {
		cf_traits::mocks::tracked_data_provider::TrackedDataProvider::<Ethereum>::set_tracked_data(
			EthereumTrackedData {
				base_fee: Default::default(),
				priority_fee: Default::default()
			}
		);
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
			.then_apply_extrinsics(move |channels| {
				channels
					.iter()
					.zip(amounts)
					.filter_map(|((request, _channel_id, deposit_address), amount)| {
						(!amount.is_zero()).then_some((
							OriginTrait::none(),
							RuntimeCall::from(pallet_cf_ingress_egress::Call::process_deposits {
								deposit_witnesses: vec![DepositWitness {
									deposit_address: *deposit_address,
									asset: request.source_asset(),
									amount,
									deposit_details: Default::default(),
								}],
								block_height: Default::default(),
							}),
							Ok(()),
						))
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
						IngressEgress::request_liquidity_deposit_address(lp_account, asset, 0)
							.map(|(id, addr, ..)| {
								(request, id, TestChainAccount::try_from(addr).unwrap())
							})
							.unwrap(),
					DepositRequest::SimpleSwap {
						source_asset,
						destination_asset,
						ref destination_address,
					} => IngressEgress::request_swap_deposit_address(
						source_asset,
						destination_asset.into(),
						destination_address.clone(),
						Default::default(),
						BROKER,
						None,
						0,
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

pub trait CheckDepositBalances {
	fn check_deposit_balances(
		self,
		expected_balances: &[(TestChainAsset, TestChainAmount)],
	) -> Self;
}

impl<Ctx: Clone> CheckDepositBalances for TestExternalities<Test, Ctx> {
	#[track_caller]
	fn check_deposit_balances(
		self,
		expected_balances: &[(TestChainAsset, TestChainAmount)],
	) -> Self {
		self.inspect_storage(|_| {
			for (asset, expected_balance) in expected_balances {
				assert_eq!(
					DepositBalances::<Test, _>::get(asset).total(),
					*expected_balance,
					"Unexpected balance for {asset:?}. Expected {expected_balance}, got {:?}.",
					DepositBalances::<Test, _>::get(asset)
				);
			}
		})
	}
}
