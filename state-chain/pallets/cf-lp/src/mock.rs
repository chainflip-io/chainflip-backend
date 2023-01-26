use crate as pallet_cf_lp;
use cf_chains::{eth::assets, AnyChain, Chain, Ethereum};
use cf_primitives::{AccountId, AccountRole, AuthorityCount, IntentId};
use cf_traits::{
	mocks::{
		bid_info::MockBidInfo, egress_handler::MockEgressHandler,
		ensure_origin_mock::NeverFailingOriginCheck, ingress_handler::MockIngressHandler,
		staking_info::MockStakingInfo, system_state_info::MockSystemStateInfo,
	},
	AddressDerivationApi,
};
use frame_support::{parameter_types, sp_runtime::app_crypto::sp_core::H160};
use frame_system as system;
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage, Permill,
};

use sp_std::str::FromStr;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

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

cf_traits::impl_mock_epoch_info!(AccountId, u128, u32, AuthorityCount);

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

impl cf_traits::Chainflip for Test {
	type KeyId = Vec<u8>;
	type ValidatorId = AccountId;
	type Amount = u128;
	type RuntimeCall = RuntimeCall;
	type EnsureWitnessed = NeverFailingOriginCheck<Self>;
	type EnsureWitnessedAtCurrentEpoch = NeverFailingOriginCheck<Self>;
	type EpochInfo = MockEpochInfo;
	type SystemState = MockSystemStateInfo;
}

impl pallet_cf_account_roles::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type BidInfo = MockBidInfo;
	type StakeInfo = MockStakingInfo<Self>;
	type EnsureGovernance = NeverFailingOriginCheck<Self>;
	type WeightInfo = ();
}

parameter_types! {
	pub const NetworkFee: Permill = Permill::from_percent(0);
}
impl pallet_cf_pools::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type NetworkFee = NetworkFee;
	type EnsureGovernance = NeverFailingOriginCheck<Self>;
}

impl crate::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type AccountRoleRegistry = AccountRoles;
	type IngressHandler = MockIngressHandler<AnyChain, Self>;
	type EgressHandler = MockEgressHandler<AnyChain>;
	type LiquidityPoolApi = LiquidityPools;
	type EnsureGovernance = NeverFailingOriginCheck<Self>;
}

pub const LP_ACCOUNT: [u8; 32] = [1u8; 32];
pub const NON_LP_ACCOUNT: [u8; 32] = [2u8; 32];

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let config = GenesisConfig {
		system: Default::default(),
		account_roles: AccountRolesConfig {
			initial_account_roles: vec![
				(LP_ACCOUNT.into(), AccountRole::LiquidityProvider),
				(NON_LP_ACCOUNT.into(), AccountRole::Validator),
			],
		},
		liquidity_pools: Default::default(),
	};

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
