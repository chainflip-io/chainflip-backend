use crate as pallet_cf_lp;
use cf_chains::{eth::assets, AnyChain, Chain, Ethereum};
use cf_primitives::{AccountRole, IntentId};
use cf_traits::{
	mocks::{
		all_batch::{MockAllBatch, MockEthEnvironment},
		bid_info::MockBidInfo,
		egress_handler::MockEgressHandler,
		ensure_origin_mock::NeverFailingOriginCheck,
		ingress_handler::MockIngressHandler,
		staking_info::MockStakingInfo,
		system_state_info::MockSystemStateInfo,
	},
	AddressDerivationApi, Broadcaster,
};
use frame_support::{parameter_types, sp_runtime::app_crypto::sp_core::H160};
use frame_system as system;
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};

use sp_std::str::FromStr;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;
type AccountId = u64;

pub struct MockAddressDerivation;

impl AddressDerivationApi<Ethereum> for MockAddressDerivation {
	fn generate_address(
		_ingress_asset: assets::eth::Asset,
		_intent_id: IntentId,
	) -> Result<<Ethereum as Chain>::ChainAccount, sp_runtime::DispatchError> {
		Ok(H160::from_str("F29aB9EbDb481BE48b80699758e6e9a3DBD609C6").unwrap())
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
		LiquidityProvider: pallet_cf_lp,
		LiquidityPools: pallet_cf_pools,
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

pub struct MockBroadcast;
impl Broadcaster<Ethereum> for MockBroadcast {
	type ApiCall = MockAllBatch<MockEthEnvironment>;

	fn threshold_sign_and_broadcast(_api_call: Self::ApiCall) {}
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

impl pallet_cf_account_roles::Config for Test {
	type Event = Event;
	type BidInfo = MockBidInfo;
	type StakeInfo = MockStakingInfo<Self>;
	type WeightInfo = ();
}

impl pallet_cf_pools::Config for Test {}

impl crate::Config for Test {
	type Event = Event;
	type AccountRoleRegistry = AccountRoles;
	type IngressHandler = MockIngressHandler<AnyChain, Self>;
	type EgressHandler = MockEgressHandler<AnyChain>;
	type LiquidityPoolApi = LiquidityPools;
	type EnsureGovernance = NeverFailingOriginCheck<Self>;
}

pub const LP_ACCOUNT: u64 = 1;
pub const NON_LP_ACCOUNT: u64 = 2;

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let config = GenesisConfig {
		system: Default::default(),
		account_roles: AccountRolesConfig {
			initial_account_roles: vec![
				(LP_ACCOUNT, AccountRole::LiquidityProvider),
				(NON_LP_ACCOUNT, AccountRole::Validator),
			],
		},
	};

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
