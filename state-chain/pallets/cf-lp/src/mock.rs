use crate::{self as pallet_cf_lp};
use cf_chains::{Chain, Ethereum};
use cf_traits::{
	impl_mock_staking_info,
	mocks::{
		bid_info::MockBidInfo, ensure_origin_mock::NeverFailingOriginCheck,
		system_state_info::MockSystemStateInfo,
	},
	AddressDerivationApi, EgressApi, StakingInfo,
};
use frame_support::{parameter_types, sp_runtime::app_crypto::sp_core::H160};
use frame_system as system;
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};

use cf_primitives::{Asset, AssetAmount, ForeignChainAddress, IntentId};

use sp_std::str::FromStr;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;
type AccountId = u64;
type Balance = u128;

pub struct MockAddressDerivation;

impl AddressDerivationApi for MockAddressDerivation {
	fn generate_address(
		_ingress_asset: Asset,
		_intent_id: IntentId,
	) -> Result<cf_primitives::ForeignChainAddress, sp_runtime::DispatchError> {
		Ok(ForeignChainAddress::Eth(
			H160::from_str("F29aB9EbDb481BE48b80699758e6e9a3DBD609C6")
				.unwrap()
				.to_fixed_bytes(),
		))
	}
}

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system,
		AccountRoles: pallet_cf_account_roles,
		Ingress: pallet_cf_ingress,
		LiquidityProvider: pallet_cf_lp,
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
	type Origin = Origin;
	type Call = Call;
	type Index = u64;
	type BlockNumber = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = Header;
	type Event = Event;
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

impl pallet_cf_ingress::Config for Test {
	type Event = Event;
	type AddressDerivation = MockAddressDerivation;
	type LpAccountHandler = LiquidityProvider;
	type IngressFetchApi = ();
	type SwapIntentHandler = Self;
	type WeightInfo = ();
}

impl cf_traits::Chainflip for Test {
	type KeyId = Vec<u8>;
	type ValidatorId = u64;
	type Amount = u128;
	type Call = Call;
	type EnsureWitnessed = NeverFailingOriginCheck<Self>;
	type EnsureWitnessedAtCurrentEpoch = NeverFailingOriginCheck<Self>;
	type EpochInfo = cf_traits::mocks::epoch_info::MockEpochInfo;
	type SystemState = MockSystemStateInfo;
}

impl_mock_staking_info!(AccountId, Balance);

impl pallet_cf_account_roles::Config for Test {
	type Event = Event;
	type BidInfo = MockBidInfo;
	type StakeInfo = MockStakingInfo;
	type WeightInfo = ();
}

parameter_types! {
	pub static LastEgress: Option<(<Ethereum as Chain>::ChainAsset, AssetAmount, <Ethereum as Chain>::ChainAccount)> = None;
}
pub struct MockEgressApi;
impl EgressApi<Ethereum> for MockEgressApi {
	fn schedule_egress(
		asset: <Ethereum as Chain>::ChainAsset,
		amount: AssetAmount,
		egress_address: <Ethereum as Chain>::ChainAccount,
	) {
		LastEgress::set(Some((asset, amount, egress_address)));
	}
}

impl crate::Config for Test {
	type Event = Event;
	type AccountRoleRegistry = AccountRoles;
	type Ingress = Ingress;
	type EgressApi = MockEgressApi;
	type EnsureGovernance = NeverFailingOriginCheck<Self>;
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let config = GenesisConfig { system: Default::default(), account_roles: Default::default() };

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
