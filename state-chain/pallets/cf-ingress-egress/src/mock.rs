use crate::DepositWitness;
pub use crate::{self as pallet_cf_ingress_egress};
pub use cf_chains::{
	address::ForeignChainAddress,
	eth::api::{EthereumApi, EthereumReplayProtection},
	CcmDepositMetadata, Chain, ChainAbi, ChainEnvironment,
};
use cf_primitives::ChannelId;
pub use cf_primitives::{
	chains::{assets, Ethereum},
	Asset, AssetAmount, EthereumAddress, ETHEREUM_ETH_ADDRESS,
};
use cf_traits::{
	impl_mock_callback, impl_mock_chainflip,
	mocks::{
		api_call::{MockEthEnvironment, MockEthereumApiCall},
		broadcaster::MockBroadcaster,
		ccm_handler::MockCcmHandler,
	},
	DepositApi, DepositHandler,
};
use frame_support::{
	parameter_types,
	traits::{OriginTrait, UnfilteredDispatchable},
};
use frame_system as system;
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup, Zero},
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
		IngressEgress: pallet_cf_ingress_egress,
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

impl_mock_chainflip!(Test);
impl_mock_callback!(RuntimeOrigin);

pub struct MockDepositHandler;
impl DepositHandler<Ethereum> for MockDepositHandler {}

pub type MockEgressBroadcaster =
	MockBroadcaster<(MockEthereumApiCall<MockEthEnvironment>, RuntimeCall)>;

impl crate::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type TargetChain = Ethereum;
	type AddressDerivation = ();
	type LpBalance = Self;
	type SwapDepositHandler = Self;
	type ChainApiCall = MockEthereumApiCall<MockEthEnvironment>;
	type Broadcaster = MockEgressBroadcaster;
	type DepositHandler = MockDepositHandler;
	type WeightInfo = ();
	type CcmHandler = MockCcmHandler;
}

pub const ALICE: <Test as frame_system::Config>::AccountId = 123u64;

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> cf_test_utilities::TestExternalities<Test, AllPalletsWithSystem> {
	cf_test_utilities::TestExternalities::<_, _>::new(GenesisConfig { system: Default::default() })
}

pub trait RequestAddressAndDeposit {
	fn request_address_and_deposit(
		self,
		requests: &[(
			<Test as frame_system::Config>::AccountId,
			<<Test as crate::Config>::TargetChain as Chain>::ChainAsset,
			<<Test as crate::Config>::TargetChain as Chain>::ChainAmount,
		)],
	) -> cf_test_utilities::TestExternalities<
		Test,
		AllPalletsWithSystem,
		Vec<(
			ChannelId,
			<<Test as crate::Config>::TargetChain as Chain>::ChainAccount,
			<<Test as crate::Config>::TargetChain as Chain>::ChainAsset,
		)>,
	>;
}

impl<Ctx: Clone> RequestAddressAndDeposit
	for cf_test_utilities::TestExternalities<Test, AllPalletsWithSystem, Ctx>
{
	fn request_address_and_deposit(
		self,
		deposit_details: &[(
			<Test as frame_system::Config>::AccountId,
			<<Test as crate::Config>::TargetChain as Chain>::ChainAsset,
			<<Test as crate::Config>::TargetChain as Chain>::ChainAmount,
		)],
	) -> cf_test_utilities::TestExternalities<
		Test,
		AllPalletsWithSystem,
		Vec<(
			ChannelId,
			<<Test as crate::Config>::TargetChain as Chain>::ChainAccount,
			<<Test as crate::Config>::TargetChain as Chain>::ChainAsset,
		)>,
	> {
		let (requests, amounts): (Vec<_>, Vec<_>) = deposit_details
			.into_iter()
			.copied()
			.map(|(acct, asset, amount)| ((acct, asset), amount))
			.unzip();

		self.request_deposit_addresses(&requests[..])
			.then_apply_extrinsics(move |channel| {
				channel
					.into_iter()
					.zip(amounts)
					.filter_map(|(&(_channel_id, ref deposit_address, asset), amount)| {
						(!amount.is_zero()).then_some((
							OriginTrait::none(),
							RuntimeCall::from(pallet_cf_ingress_egress::Call::process_deposits {
								deposit_witnesses: vec![DepositWitness {
									deposit_address: deposit_address.clone().try_into().unwrap(),
									asset: asset.into(),
									amount,
									tx_id: Default::default(),
								}],
							}),
						))
					})
					.collect::<Vec<_>>()
			})
			.map_context(|(deposit_details, extrinsic_results)| {
				for (call, result) in extrinsic_results {
					assert!(result.is_ok(), "Extrinsic failed: {:?}", call);
				}
				deposit_details
			})
	}
}

pub trait RequestAddress {
	fn request_deposit_addresses(
		self,
		requests: &[(
			<Test as frame_system::Config>::AccountId,
			<<Test as crate::Config>::TargetChain as Chain>::ChainAsset,
		)],
	) -> cf_test_utilities::TestExternalities<
		Test,
		AllPalletsWithSystem,
		Vec<(
			ChannelId,
			<<Test as crate::Config>::TargetChain as Chain>::ChainAccount,
			<<Test as crate::Config>::TargetChain as Chain>::ChainAsset,
		)>,
	>;
}

impl<Ctx: Clone> RequestAddress
	for cf_test_utilities::TestExternalities<Test, AllPalletsWithSystem, Ctx>
{
	fn request_deposit_addresses(
		self,
		requests: &[(
			<Test as frame_system::Config>::AccountId,
			<<Test as crate::Config>::TargetChain as Chain>::ChainAsset,
		)],
	) -> cf_test_utilities::TestExternalities<
		Test,
		AllPalletsWithSystem,
		Vec<(
			ChannelId,
			<<Test as crate::Config>::TargetChain as Chain>::ChainAccount,
			<<Test as crate::Config>::TargetChain as Chain>::ChainAsset,
		)>,
	> {
		self.then_execute_as_next_block(|_| {
			requests
				.into_iter()
				.copied()
				.map(|(broker, asset)| {
					IngressEgress::request_liquidity_deposit_address(broker, asset)
						.map(|(id, addr)| {
							(id, <<Test as crate::Config>::TargetChain as Chain>::ChainAccount::try_from(addr).unwrap(), asset)
						})
						.unwrap()
				})
				.collect()
		})
	}
}
