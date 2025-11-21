// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use crate::{
	self as pallet_cf_ingress_egress, Config, DepositWitness, Pallet, PalletSafeMode,
	WhitelistedBrokers,
};
use cf_chains::{
	address::{AddressDerivationApi, AddressDerivationError},
	assets,
	btc::{deposit_address::DepositAddress, BitcoinTrackedData},
	eth::EthereumTrackedData,
	Bitcoin, Chain, ChannelRefundParametersForChain, Ethereum, ForeignChainAddress,
};
use cf_primitives::ChannelId;
use cf_test_utilities::{impl_test_helpers, TestExternalities};
use cf_traits::{
	impl_mock_chainflip, impl_mock_runtime_safe_mode,
	mocks::{
		address_converter::MockAddressConverter,
		affiliate_registry::MockAffiliateRegistry,
		api_call::{
			MockBitcoinApiCall, MockBtcEnvironment, MockEthereumApiCall, MockEvmEnvironment,
		},
		asset_converter::MockAssetConverter,
		asset_withholding::MockAssetWithholding,
		balance_api::{MockBalance, MockLpRegistration},
		broadcaster::MockBroadcaster,
		ccm_additional_data_handler::MockCcmAdditionalDataHandler,
		chain_tracking::ChainTracker,
		fee_payment::MockFeePayment,
		fetches_transfers_limit_provider::MockFetchesTransfersLimitProvider,
		lending_pools::MockBoostApi,
		swap_parameter_validation::MockSwapParameterValidation,
		swap_request_api::MockSwapRequestHandler,
	},
	AccountRoleRegistry, DepositApi, DummyIngressSource, NetworkEnvironmentProvider, OnDeposit,
};
use frame_support::{
	assert_ok, derive_impl,
	instances::{Instance1, Instance2},
	sp_runtime::traits::Zero,
};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_core::{ConstBool, ConstU64, U256};

type AccountId = u64;
type Block = frame_system::mocking::MockBlock<Test>;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

pub type MockEgressBroadcasterEth =
	MockBroadcaster<(MockEthereumApiCall<MockEvmEnvironment>, RuntimeCall)>;

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
		Ok((
			<Self as AddressDerivationApi<Ethereum>>::generate_address(source_asset, channel_id)?,
			Default::default(),
		))
	}
}

pub type MockEgressBroadcasterBtc =
	MockBroadcaster<(MockBitcoinApiCall<MockBtcEnvironment>, RuntimeCall)>;

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
		Ok((
			<Self as AddressDerivationApi<Bitcoin>>::generate_address(source_asset, channel_id)?,
			DepositAddress::new([1u8; 32], 123),
		))
	}
}

impl Config<Instance1> for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	const MANAGE_CHANNEL_LIFETIME: bool = true;
	const ONLY_PREALLOCATE_FROM_POOL: bool = true;
	type IngressSource = DummyIngressSource<Ethereum, BlockNumberFor<Self>>;
	type TargetChain = Ethereum;
	type AddressDerivation = MockAddressDerivation;
	type AddressConverter = MockAddressConverter;
	type Balance = MockBalance;
	type ChainApiCall = MockEthereumApiCall<MockEvmEnvironment>;
	type Broadcaster = MockEgressBroadcasterEth;
	type DepositHandler = MockDepositHandler;
	type ChainTracking = ChainTracker<Ethereum>;
	type WeightInfo = ();
	type NetworkEnvironment = MockNetworkEnvironmentProvider;
	type AssetConverter = MockAssetConverter;
	type FeePayment = MockFeePayment<Self>;
	type SwapRequestHandler = MockSwapRequestHandler<(Ethereum, crate::Pallet<Self, Instance1>)>;
	type AssetWithholding = MockAssetWithholding;
	type FetchesTransfersLimitProvider = MockFetchesTransfersLimitProvider;
	type SafeMode = MockRuntimeSafeMode;
	type SwapParameterValidation = MockSwapParameterValidation;
	type CcmAdditionalDataHandler = MockCcmAdditionalDataHandler;
	type AffiliateRegistry = MockAffiliateRegistry;
	type AllowTransactionReports = ConstBool<true>;
	type ScreeningBrokerId = ConstU64<SCREENING_ID>;
	type BoostApi = MockBoostApi;
	type FundAccount = MockFundingInfo<Test>;
	type LpRegistrationApi = MockLpRegistration;
}

impl Config<Instance2> for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	const MANAGE_CHANNEL_LIFETIME: bool = true;
	const ONLY_PREALLOCATE_FROM_POOL: bool = false;
	type IngressSource = DummyIngressSource<Bitcoin, BlockNumberFor<Self>>;
	type TargetChain = Bitcoin;
	type AddressDerivation = MockAddressDerivation;
	type AddressConverter = MockAddressConverter;
	type Balance = MockBalance;
	type ChainApiCall = MockBitcoinApiCall<MockBtcEnvironment>;
	type Broadcaster = MockEgressBroadcasterBtc;
	type DepositHandler = MockDepositHandler;
	type ChainTracking = ChainTracker<Bitcoin>;
	type WeightInfo = ();
	type NetworkEnvironment = MockNetworkEnvironmentProvider;
	type AssetConverter = MockAssetConverter;
	type FeePayment = MockFeePayment<Self>;
	type SwapRequestHandler = MockSwapRequestHandler<(Bitcoin, crate::Pallet<Self, Instance2>)>;
	type AssetWithholding = MockAssetWithholding;
	type FetchesTransfersLimitProvider = cf_traits::NoLimit;
	type SafeMode = MockRuntimeSafeMode;
	type SwapParameterValidation = MockSwapParameterValidation;
	type CcmAdditionalDataHandler = MockCcmAdditionalDataHandler;
	type AffiliateRegistry = MockAffiliateRegistry;
	type AllowTransactionReports = ConstBool<true>;
	type ScreeningBrokerId = ConstU64<SCREENING_ID>;
	type BoostApi = MockBoostApi;
	type FundAccount = MockFundingInfo<Test>;
	type LpRegistrationApi = MockLpRegistration;
}

impl_mock_chainflip!(Test);

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		EthereumIngressEgress: pallet_cf_ingress_egress::<Instance1>,
		BitcoinIngressEgress: pallet_cf_ingress_egress::<Instance2>,
	}
);

pub struct MockDepositHandler;
impl<C: Chain> OnDeposit<C> for MockDepositHandler {}

pub struct MockAddressDerivation;

pub struct MockNetworkEnvironmentProvider {}

impl NetworkEnvironmentProvider for MockNetworkEnvironmentProvider {
	fn get_network_environment() -> cf_primitives::NetworkEnvironment {
		cf_primitives::NetworkEnvironment::Development
	}
}

impl_mock_runtime_safe_mode! {
	ingress_egress_ethereum: PalletSafeMode<Instance1>,
	ingress_egress_bitcoin: PalletSafeMode<Instance2>,
}

pub const ALICE: <Test as frame_system::Config>::AccountId = 123u64;
pub const BROKER: <Test as frame_system::Config>::AccountId = 456u64;
pub const WHITELISTED_BROKER: <Test as frame_system::Config>::AccountId = BROKER + 1;
pub const SCREENING_ID: <Test as frame_system::Config>::AccountId = 0xcf;

type TestChainAccount<I> = <<Test as Config<I>>::TargetChain as Chain>::ChainAccount;
type TestChainAmount<I> = <<Test as Config<I>>::TargetChain as Chain>::ChainAmount;
type TestChainAsset<I> = <<Test as Config<I>>::TargetChain as Chain>::ChainAsset;

impl_test_helpers! {
	Test,
	RuntimeGenesisConfig {
		system: Default::default(),
		ethereum_ingress_egress: EthereumIngressEgressConfig {
			deposit_channel_lifetime: 100,
			witness_safety_margin: Some(2),
			dust_limits: Default::default(),
		},
		bitcoin_ingress_egress: BitcoinIngressEgressConfig {
			deposit_channel_lifetime: 100,
			witness_safety_margin: Some(2),
			dust_limits: Default::default(),
		},
	},
	|| {
		cf_traits::mocks::tracked_data_provider::TrackedDataProvider::<Bitcoin>::set_tracked_data(
			BitcoinTrackedData { btc_fee_info: Default::default() }
		);
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
		WhitelistedBrokers::<Test, Instance1>::insert(WHITELISTED_BROKER, ());
		WhitelistedBrokers::<Test, Instance2>::insert(WHITELISTED_BROKER, ());
	}
}

#[expect(clippy::type_complexity)]
pub trait RequestAddressAndDeposit {
	fn request_address_and_deposit<I: 'static + Clone>(
		self,
		requests: &[(DepositRequest<I>, TestChainAmount<I>)],
	) -> TestExternalities<Test, Vec<(DepositRequest<I>, ChannelId, TestChainAccount<I>)>>
	where
		Test: Config<I>,
		<<Test as Config<I>>::TargetChain as Chain>::DepositDetails: Default;
}

impl<Ctx: Clone> RequestAddressAndDeposit for TestRunner<Ctx> {
	/// Request deposit addresses and complete the deposit of funds into those addresses.
	#[track_caller]
	fn request_address_and_deposit<I: 'static + Clone>(
		self,
		requests: &[(DepositRequest<I>, TestChainAmount<I>)],
	) -> TestExternalities<Test, Vec<(DepositRequest<I>, ChannelId, TestChainAccount<I>)>>
	where
		Test: Config<I>,
		<<Test as Config<I>>::TargetChain as Chain>::DepositDetails: Default,
	{
		let (requests, amounts): (Vec<_>, Vec<_>) = requests.iter().cloned().unzip();

		self.request_deposit_addresses(&requests[..])
			.then_execute_at_next_block(move |channels| {
				channels
					.into_iter()
					.zip(amounts)
					.map(|((request, channel_id, deposit_address), amount)| {
						if !amount.is_zero() {
							Pallet::<Test, I>::process_channel_deposit_full_witness_inner(
								&DepositWitness {
									deposit_address: deposit_address.clone(),
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
pub enum DepositRequest<I: 'static>
where
	Test: Config<I>,
{
	Liquidity {
		lp_account: AccountId,
		asset: TestChainAsset<I>,
	},
	/// Do a non-ccm swap using a default broker and no fees.
	SimpleSwap {
		source_asset: TestChainAsset<I>,
		destination_asset: TestChainAsset<I>,
		destination_address: ForeignChainAddress,
		refund_address: TestChainAccount<I>,
	},
}

impl<I: 'static> DepositRequest<I>
where
	Test: Config<I>,
{
	pub fn source_asset(&self) -> TestChainAsset<I> {
		match self {
			Self::Liquidity { asset, .. } => *asset,
			Self::SimpleSwap { source_asset, .. } => *source_asset,
		}
	}
}

#[expect(clippy::type_complexity)]
pub trait RequestAddress {
	fn request_deposit_addresses<I: 'static + Clone>(
		self,
		requests: &[DepositRequest<I>],
	) -> TestExternalities<Test, Vec<(DepositRequest<I>, ChannelId, TestChainAccount<I>)>>
	where
		Test: Config<I>;
}

impl<Ctx: Clone> RequestAddress for TestExternalities<Test, Ctx> {
	#[track_caller]
	fn request_deposit_addresses<I: 'static + Clone>(
		self,
		requests: &[DepositRequest<I>],
	) -> TestExternalities<Test, Vec<(DepositRequest<I>, ChannelId, TestChainAccount<I>)>>
	where
		Test: Config<I>,
	{
		self.then_execute_with(|_| {
			#[expect(clippy::redundant_iter_cloned)]
			requests
				.iter()
				.cloned()
				.map(|request| match request {
					DepositRequest::Liquidity { lp_account, asset } =>
						Pallet::<Test, I>::request_liquidity_deposit_address(
							lp_account,
							lp_account,
							asset,
							0,
							ForeignChainAddress::Eth(Default::default()),
							None,
						)
						.map(|(id, addr, ..)| {
							(
								request,
								id,
								TestChainAccount::try_from(addr)
									.unwrap_or_else(|_| panic!("Invalid address")),
							)
						})
						.unwrap(),
					DepositRequest::SimpleSwap {
						source_asset,
						destination_asset,
						ref destination_address,
						ref refund_address,
					} => Pallet::<Test, I>::request_swap_deposit_address(
						source_asset,
						destination_asset.into(),
						destination_address.clone(),
						Default::default(),
						BROKER,
						None,
						10,
						ChannelRefundParametersForChain::<<Test as Config<I>>::TargetChain> {
							retry_duration: 5,
							refund_address: refund_address.clone(),
							min_price: U256::zero(),
							refund_ccm_metadata: None,
							max_oracle_price_slippage: None,
						},
						None,
					)
					.map(|(channel_id, deposit_address, ..)| {
						(
							request,
							channel_id,
							TestChainAccount::try_from(deposit_address)
								.unwrap_or_else(|_| panic!("Invalid address")),
						)
					})
					.unwrap(),
				})
				.collect()
		})
	}
}
