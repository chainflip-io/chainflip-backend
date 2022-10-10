use crate::{self as pallet_cf_lp};
use cf_traits::{
	mocks::{ensure_origin_mock::NeverFailingOriginCheck, system_state_info::MockSystemStateInfo},
	AddressDerivationApi, EgressApi,
};
use frame_support::{
	dispatch::DispatchResult, parameter_types, sp_runtime::app_crypto::sp_core::H160,
};
use frame_system as system;
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};

use cf_primitives::{AssetAmount, ForeignChainAddress, ForeignChainAsset, IntentId};

use sp_std::str::FromStr;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;
type AccountId = u64;

pub struct MockAddressDerivation;

impl AddressDerivationApi for MockAddressDerivation {
	fn generate_address(
		_ingress_asset: ForeignChainAsset,
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
		AccountTypes: pallet_cf_account_types,
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

impl pallet_cf_account_types::Config for Test {
	type Event = Event;
}

parameter_types! {
	pub static IsValid: bool = false;
	pub static LastEgress: Option<(ForeignChainAsset, AssetAmount, ForeignChainAddress)> = None;
}
pub struct MockEgressApi;
impl EgressApi for MockEgressApi {
	fn schedule_egress(
		foreign_asset: ForeignChainAsset,
		amount: AssetAmount,
		egress_address: ForeignChainAddress,
	) -> DispatchResult {
		LastEgress::set(Some((foreign_asset, amount, egress_address)));
		Ok(())
	}

	fn is_egress_valid(
		_foreign_asset: &ForeignChainAsset,
		_egress_address: &ForeignChainAddress,
	) -> bool {
		IsValid::get()
	}
}

impl crate::Config for Test {
	type Event = Event;
	type AccountRoleRegistry = AccountTypes;
	type Ingress = Ingress;
	type EgressApi = MockEgressApi;
	type EnsureGovernance = NeverFailingOriginCheck<Self>;
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let config = GenesisConfig { system: Default::default(), account_types: Default::default() };

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
