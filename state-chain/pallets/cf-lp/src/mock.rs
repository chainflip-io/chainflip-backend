use crate as pallet_cf_lp;
use cf_chains::{eth::assets, AnyChain, Chain, Ethereum};
use cf_primitives::{AccountId, IntentId};
use cf_traits::{
	impl_mock_chainflip,
	mocks::{
		address_converter::MockAddressConverter, egress_handler::MockEgressHandler,
		ingress_handler::MockIngressHandler,
	},
	AccountRoleRegistry, AddressDerivationApi,
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

parameter_types! {
	pub const NetworkFee: Permill = Permill::from_percent(0);
}

impl crate::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type IngressHandler = MockIngressHandler<AnyChain, Self>;
	type EgressHandler = MockEgressHandler<AnyChain>;
	type AddressConverter = MockAddressConverter;
	type WeightInfo = ();
}

pub const LP_ACCOUNT: [u8; 32] = [1u8; 32];
pub const NON_LP_ACCOUNT: [u8; 32] = [2u8; 32];

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut ext: sp_io::TestExternalities =
		GenesisConfig::default().build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
		<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_liquidity_provider(
			&LP_ACCOUNT.into(),
		)
		.unwrap();
		<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_validator(
			&NON_LP_ACCOUNT.into(),
		)
		.unwrap();
	});

	ext
}
