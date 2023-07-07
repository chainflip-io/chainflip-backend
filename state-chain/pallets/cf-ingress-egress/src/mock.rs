use crate::DepositWitness;
pub use crate::{self as pallet_cf_ingress_egress};
use cf_chains::eth::EthereumChannelId;
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
	AddressDerivationApi, DepositApi, DepositChannel, DepositHandler, GetBlockHeight,
};
use codec::{Decode, Encode};
use frame_support::traits::{OriginTrait, UnfilteredDispatchable};
use frame_system as system;
use scale_info::TypeInfo;
use sp_core::{H160, H256};
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup, Zero},
};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;
type AccountId = u64;

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
pub mod eth_mock_deposit_channel {
	use super::*;

	#[derive(Encode, Decode, TypeInfo, Clone, PartialEq, Eq, Copy, Debug)]
	pub enum DeploymentStatus {
		Deployed,
		Pending,
		Undeployed,
	}

	#[derive(Encode, Decode, TypeInfo, Clone, PartialEq, Eq, Copy, Debug)]
	pub struct MockDepositChannel {
		pub address: H160,
		pub channel_id: u64,
		pub deployment_status: DeploymentStatus,
		pub deposit_fetch_id: EthereumChannelId,
		pub asset: <Ethereum as Chain>::ChainAsset,
	}

	impl DepositChannel<Ethereum> for MockDepositChannel {
		type Address = H160;
		type DepositFetchId = EthereumChannelId;
		type AddressDerivation = ();

		fn get_address(&self) -> Self::Address {
			self.address
		}

		fn process_broadcast(mut self) -> (Self, bool)
		where
			Self: Sized,
		{
			match self.deployment_status {
				DeploymentStatus::Deployed => (self, true),
				DeploymentStatus::Pending => (self, false),
				DeploymentStatus::Undeployed => {
					self.deployment_status = DeploymentStatus::Pending;
					(self, true)
				},
			}
		}

		fn get_deposit_fetch_id(&self) -> Self::DepositFetchId {
			self.deposit_fetch_id
		}

		fn new(
			channel_id: u64,
			asset: <Ethereum as Chain>::ChainAsset,
		) -> Result<MockDepositChannel, sp_runtime::DispatchError> {
			let address =
				<() as AddressDerivationApi<Ethereum>>::generate_address(asset, channel_id)?;
			Ok(Self {
				address,
				channel_id,
				asset,
				deployment_status: DeploymentStatus::Undeployed,
				deposit_fetch_id: EthereumChannelId::UnDeployed(channel_id),
			})
		}

		fn maybe_recycle(&self) -> bool
		where
			Self: Sized,
		{
			self.deployment_status == DeploymentStatus::Deployed
		}

		fn finalize(mut self) -> Self
		where
			Self: Sized,
		{
			match self.deployment_status {
				DeploymentStatus::Pending => {
					self.deposit_fetch_id = EthereumChannelId::Deployed(self.address);
					self.deployment_status = DeploymentStatus::Deployed;
				},
				DeploymentStatus::Undeployed => self.deployment_status = DeploymentStatus::Pending,
				_ => (),
			}
			self
		}

		fn get_channel_id(&self) -> u64 {
			self.channel_id
		}

		fn get_asset(&self) -> <Ethereum as Chain>::ChainAsset {
			self.asset
		}
	}
}

pub struct BlockNumberProvider;

pub const OPEN_INGRESS_AT: u64 = 420;

impl GetBlockHeight<Ethereum> for BlockNumberProvider {
	fn get_block_height() -> u64 {
		OPEN_INGRESS_AT
	}
}

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
	type CcmHandler = MockCcmHandler;
	type DepositChannel = eth_mock_deposit_channel::MockDepositChannel;
	type ChainTracking = BlockNumberProvider;
	type WeightInfo = ();
}

pub const ALICE: <Test as frame_system::Config>::AccountId = 123u64;

// Configure a mock runtime to test the pallet.
cf_test_utilities::impl_test_helpers!(Test);

type TestChainAccount = <<Test as crate::Config>::TargetChain as Chain>::ChainAccount;
type TestChainAmount = <<Test as crate::Config>::TargetChain as Chain>::ChainAmount;
type TestChainAsset = <<Test as crate::Config>::TargetChain as Chain>::ChainAsset;

pub trait RequestAddressAndDeposit {
	fn request_address_and_deposit(
		self,
		requests: &[(<Test as frame_system::Config>::AccountId, TestChainAsset, TestChainAmount)],
	) -> cf_test_utilities::TestExternalities<
		Test,
		AllPalletsWithSystem,
		Vec<(ChannelId, TestChainAccount, TestChainAsset)>,
	>;
}

impl<Ctx: Clone> RequestAddressAndDeposit for TestRunner<Ctx> {
	fn request_address_and_deposit(
		self,
		deposit_details: &[(
			<Test as frame_system::Config>::AccountId,
			TestChainAsset,
			TestChainAmount,
		)],
	) -> cf_test_utilities::TestExternalities<
		Test,
		AllPalletsWithSystem,
		Vec<(ChannelId, TestChainAccount, TestChainAsset)>,
	> {
		let (requests, amounts): (Vec<_>, Vec<_>) = deposit_details
			.iter()
			.copied()
			.map(|(acct, asset, amount)| ((acct, asset), amount))
			.unzip();

		self.request_deposit_addresses(&requests[..])
			.then_apply_extrinsics(move |channel| {
				channel
					.iter()
					.zip(amounts)
					.filter_map(|(&(_channel_id, deposit_address, asset), amount)| {
						(!amount.is_zero()).then_some((
							OriginTrait::none(),
							RuntimeCall::from(pallet_cf_ingress_egress::Call::process_deposits {
								deposit_witnesses: vec![DepositWitness {
									deposit_address,
									asset,
									amount,
									tx_id: Default::default(),
								}],
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
		requests: &[(<Test as frame_system::Config>::AccountId, TestChainAsset)],
	) -> cf_test_utilities::TestExternalities<
		Test,
		AllPalletsWithSystem,
		Vec<(ChannelId, TestChainAccount, TestChainAsset)>,
	>;
}

impl<Ctx: Clone> RequestAddress
	for cf_test_utilities::TestExternalities<Test, AllPalletsWithSystem, Ctx>
{
	fn request_deposit_addresses(
		self,
		requests: &[(<Test as frame_system::Config>::AccountId, TestChainAsset)],
	) -> cf_test_utilities::TestExternalities<
		Test,
		AllPalletsWithSystem,
		Vec<(ChannelId, TestChainAccount, TestChainAsset)>,
	> {
		self.then_execute_at_next_block(|_| {
			requests
				.iter()
				.copied()
				.map(|(broker, asset)| {
					IngressEgress::request_liquidity_deposit_address(broker, asset)
						.map(|(id, addr)| (id, TestChainAccount::try_from(addr).unwrap(), asset))
						.unwrap()
				})
				.collect()
		})
	}
}
