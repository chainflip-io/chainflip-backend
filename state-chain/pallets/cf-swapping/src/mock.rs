use crate::{self as pallet_cf_swapping, Pallet};
use cf_chains::{Chain, Ethereum};
use cf_primitives::{chains::assets, Asset, AssetAmount, ForeignChainAddress};
use cf_traits::{
	mocks::{ensure_origin_mock::NeverFailingOriginCheck, system_state_info::MockSystemStateInfo},
	Chainflip, EgressApi, IngressApi, SwappingApi,
};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{parameter_types, storage_alias};
use frame_system as system;
use scale_info::TypeInfo;
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};

pub const RELAYER_FEE: u128 = 5;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;
type AccountId = u64;

/// A helper type for testing
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy)]
pub struct EgressTransaction {
	pub asset: assets::eth::Asset,
	pub amount: AssetAmount,
	pub egress_address: <Ethereum as Chain>::ChainAccount,
}

#[storage_alias]
pub type EgressQueue<T: crate::pallet::Config> = StorageValue<Pallet<T>, Vec<EgressTransaction>>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system,
		Swapping: pallet_cf_swapping,
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

pub struct MockSwappingApi;

impl SwappingApi for MockSwappingApi {
	fn swap(
		_from: Asset,
		_to: Asset,
		swap_input: AssetAmount,
		_fee: u16,
	) -> (AssetAmount, (cf_primitives::Asset, AssetAmount)) {
		(swap_input, (cf_primitives::Asset::Usdc, RELAYER_FEE))
	}
}

impl Chainflip for Test {
	type KeyId = Vec<u8>;
	type ValidatorId = u64;
	type Amount = u128;
	type Call = Call;
	type EnsureWitnessed = NeverFailingOriginCheck<Self>;
	type EnsureWitnessedAtCurrentEpoch = NeverFailingOriginCheck<Self>;
	type EpochInfo = cf_traits::mocks::epoch_info::MockEpochInfo;
	type SystemState = MockSystemStateInfo;
}

impl pallet_cf_swapping::Config for Test {
	type Event = Event;
	type AccountRoleRegistry = ();
	type IngressHandler = MockIngressHandler<AnyChain, Self>;
	type EgressHandler = MockEgressHandler<AnyChain, Self>;
	type SwappingApi = MockSwappingApi;
	type WeightInfo = ();
}

pub const ALICE: <Test as frame_system::Config>::AccountId = 123u64;

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let config = GenesisConfig { system: Default::default() };

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
