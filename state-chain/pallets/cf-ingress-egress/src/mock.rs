use crate::DepositWitness;
pub use crate::{self as pallet_cf_ingress_egress};
pub use cf_chains::{
	address::{AddressDerivationApi, ForeignChainAddress},
	eth::{
		api::{EthereumApi, EthereumReplayProtection},
		Address as EthereumAddress,
	},
	CcmDepositMetadata, Chain, ChainAbi, ChainEnvironment, DepositChannel,
};
use cf_primitives::ChannelId;
pub use cf_primitives::{
	chains::{assets, Ethereum},
	Asset, AssetAmount,
};
use cf_test_utilities::{impl_test_helpers, TestExternalities};
use cf_traits::{
	impl_mock_callback, impl_mock_chainflip,
	mocks::{
		address_converter::MockAddressConverter,
		api_call::{MockEthEnvironment, MockEthereumApiCall},
		broadcaster::MockBroadcaster,
		ccm_handler::MockCcmHandler,
	},
	DepositApi, DepositHandler, GetBlockHeight,
};
use frame_support::traits::{OriginTrait, UnfilteredDispatchable};
use frame_system as system;
use frame_system::pallet_prelude::BlockNumberFor;
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

pub struct BlockNumberProvider;

pub const OPEN_INGRESS_AT: u64 = 420;

impl GetBlockHeight<Ethereum> for BlockNumberProvider {
	fn get_block_height() -> u64 {
		OPEN_INGRESS_AT
	}
}

pub struct MockAddressDerivation;

impl AddressDerivationApi<Ethereum> for MockAddressDerivation {
	fn generate_address(
		_source_asset: assets::eth::Asset,
		channel_id: ChannelId,
	) -> Result<<Ethereum as Chain>::ChainAccount, sp_runtime::DispatchError> {
		Ok([channel_id as u8; 20].into())
	}

	fn generate_address_and_state(
		source_asset: <Ethereum as Chain>::ChainAsset,
		channel_id: ChannelId,
	) -> Result<
		(<Ethereum as Chain>::ChainAccount, <Ethereum as Chain>::DepositChannelState),
		sp_runtime::DispatchError,
	> {
		Ok((Self::generate_address(source_asset, channel_id)?, Default::default()))
	}
}

impl crate::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type TargetChain = Ethereum;
	type AddressDerivation = MockAddressDerivation;
	type AddressConverter = MockAddressConverter;
	type LpBalance = Self;
	type SwapDepositHandler = Self;
	type ChainApiCall = MockEthereumApiCall<MockEthEnvironment>;
	type Broadcaster = MockEgressBroadcaster;
	type DepositHandler = MockDepositHandler;
	type CcmHandler = MockCcmHandler;
	type ChainTracking = BlockNumberProvider;
	type WeightInfo = ();
}

pub const ALICE: <Test as frame_system::Config>::AccountId = 123u64;

type TestChainAccount = <<Test as crate::Config>::TargetChain as Chain>::ChainAccount;
type TestChainAmount = <<Test as crate::Config>::TargetChain as Chain>::ChainAmount;
type TestChainAsset = <<Test as crate::Config>::TargetChain as Chain>::ChainAsset;

pub trait RequestAddressAndDeposit {
	fn request_address_and_deposit(
		self,
		requests: &[(
			<Test as frame_system::Config>::AccountId,
			TestChainAsset,
			TestChainAmount,
			BlockNumberFor<Test>,
		)],
	) -> TestExternalities<Test, Vec<(ChannelId, TestChainAccount, TestChainAsset)>>;
}

impl<Ctx: Clone> RequestAddressAndDeposit for TestRunner<Ctx> {
	fn request_address_and_deposit(
		self,
		deposit_details: &[(
			<Test as frame_system::Config>::AccountId,
			TestChainAsset,
			TestChainAmount,
			BlockNumberFor<Test>,
		)],
	) -> TestExternalities<Test, Vec<(ChannelId, TestChainAccount, TestChainAsset)>> {
		let (requests, amounts): (Vec<_>, Vec<_>) = deposit_details
			.iter()
			.copied()
			.map(|(acct, asset, amount, expiry)| ((acct, asset, expiry), amount))
			.unzip();

		self.request_deposit_addresses(&requests[..])
			.then_apply_extrinsics(move |channel| {
				channel
					.iter()
					.zip(amounts)
					.filter_map(|(&(_channel_id, deposit_address, asset, ..), amount)| {
						(!amount.is_zero()).then_some((
							OriginTrait::none(),
							RuntimeCall::from(pallet_cf_ingress_egress::Call::process_deposits {
								deposit_witnesses: vec![DepositWitness {
									deposit_address,
									asset,
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

pub trait RequestAddress {
	fn request_deposit_addresses(
		self,
		requests: &[(
			<Test as frame_system::Config>::AccountId,
			TestChainAsset,
			BlockNumberFor<Test>,
		)],
	) -> TestExternalities<Test, Vec<(ChannelId, TestChainAccount, TestChainAsset)>>;
}

impl<Ctx: Clone> RequestAddress for TestExternalities<Test, Ctx> {
	fn request_deposit_addresses(
		self,
		requests: &[(
			<Test as frame_system::Config>::AccountId,
			TestChainAsset,
			BlockNumberFor<Test>,
		)],
	) -> TestExternalities<Test, Vec<(ChannelId, TestChainAccount, TestChainAsset)>> {
		self.then_execute_at_next_block(|_| {
			requests
				.iter()
				.copied()
				.map(|(broker, asset, expiry)| {
					IngressEgress::request_liquidity_deposit_address(broker, asset, expiry)
						.map(|(id, addr)| (id, TestChainAccount::try_from(addr).unwrap(), asset))
						.unwrap()
				})
				.collect()
		})
	}
}

impl_test_helpers!(Test);
