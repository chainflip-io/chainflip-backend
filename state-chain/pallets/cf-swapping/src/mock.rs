use core::cell::Cell;

use crate::{self as pallet_cf_swapping, PalletSafeMode, WeightInfo};
use cf_chains::{ccm_checker::CcmValidityCheck, AnyChain};
use cf_primitives::{Asset, AssetAmount};
#[cfg(feature = "runtime-benchmarks")]
use cf_traits::mocks::fee_payment::MockFeePayment;
use cf_traits::{
	impl_mock_chainflip, impl_mock_runtime_safe_mode,
	mocks::{
		address_converter::MockAddressConverter, balance_api::MockBalance,
		deposit_handler::MockDepositHandler, egress_handler::MockEgressHandler,
		ingress_egress_fee_handler::MockIngressEgressFeeHandler,
	},
	AccountRoleRegistry, SwappingApi,
};
use frame_support::{derive_impl, pallet_prelude::DispatchError, parameter_types, weights::Weight};
use frame_system as system;
use sp_core::{ConstU32, H256};
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	BoundedBTreeMap, Permill,
};

type AccountId = u64;
type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Swapping: pallet_cf_swapping,
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
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
impl_mock_runtime_safe_mode! { swapping: PalletSafeMode }

// NOTE: the use of u128 lets us avoid type conversions in tests:
pub const DEFAULT_SWAP_RATE: u128 = 2;

parameter_types! {
	pub static NetworkFee: Permill = Permill::from_perthousand(0);
	pub static Swaps: Vec<(Asset, Asset, AssetAmount)> = vec![];
	pub static SwapRate: f64 = DEFAULT_SWAP_RATE as f64;
	pub storage Liquidity: BoundedBTreeMap<Asset, AssetAmount, ConstU32<100>> = Default::default();
}

thread_local! {
	pub static SWAPS_SHOULD_FAIL: Cell<bool> = Cell::new(false);
}

pub struct MockSwappingApi;

impl MockSwappingApi {
	pub fn set_swaps_should_fail(should_fail: bool) {
		SWAPS_SHOULD_FAIL.with(|cell| cell.set(should_fail));
	}

	fn swaps_should_fail() -> bool {
		SWAPS_SHOULD_FAIL.with(|cell| cell.get())
	}

	pub fn add_liquidity(asset: Asset, amount: AssetAmount) {
		let liquidity = Liquidity::get()
			.try_mutate(|liquidity| {
				*liquidity.entry(asset).or_default() += amount;
			})
			.unwrap();

		Liquidity::set(&liquidity);
	}

	pub fn get_liquidity(asset: &Asset) -> AssetAmount {
		*Liquidity::get().get(asset).expect("liquidity not initialised for asset")
	}
}

impl SwappingApi for MockSwappingApi {
	fn swap_single_leg(
		from: Asset,
		to: Asset,
		input_amount: AssetAmount,
	) -> Result<AssetAmount, DispatchError> {
		if Self::swaps_should_fail() {
			return Err(DispatchError::from("Test swap failed"))
		}

		let mut swaps = Swaps::get();
		swaps.push((from, to, input_amount));
		Swaps::set(swaps);

		let output_amount = (input_amount as f64 * SwapRate::get()) as AssetAmount;

		let mut liquidity = Liquidity::get();

		// We only check/update liquidity if it has been initialised
		// (i.e. it is not checked in tests that don't use it):
		if let Some(asset_liquidity) = liquidity.get_mut(&to) {
			if let Some(remaining) = asset_liquidity.checked_sub(output_amount) {
				*asset_liquidity = remaining;
			} else {
				return Err(DispatchError::from("Insufficient liquidity"))
			}
		}

		if let Some(asset_liquidity) = liquidity.get_mut(&from) {
			*asset_liquidity += input_amount;
		}

		Liquidity::set(&liquidity);

		Ok(output_amount)
	}
}

pub struct MockWeightInfo;

impl WeightInfo for MockWeightInfo {
	fn request_swap_deposit_address() -> Weight {
		Weight::from_parts(100, 0)
	}

	fn request_swap_deposit_address_with_affiliates() -> Weight {
		Weight::from_parts(100, 0)
	}

	fn withdraw() -> Weight {
		Weight::from_parts(100, 0)
	}

	fn schedule_swap_from_contract() -> Weight {
		Weight::from_parts(100, 0)
	}

	fn ccm_deposit() -> Weight {
		Weight::from_parts(100, 0)
	}

	fn register_as_broker() -> Weight {
		Weight::from_parts(100, 0)
	}

	fn deregister_as_broker() -> Weight {
		Weight::from_parts(100, 0)
	}
}

pub struct AlwaysValid;
impl CcmValidityCheck for AlwaysValid {}

impl pallet_cf_swapping::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type DepositHandler = MockDepositHandler<AnyChain, Self>;
	type EgressHandler = MockEgressHandler<AnyChain>;
	type AddressConverter = MockAddressConverter;
	type SwappingApi = MockSwappingApi;
	type SafeMode = MockRuntimeSafeMode;
	type WeightInfo = MockWeightInfo;
	#[cfg(feature = "runtime-benchmarks")]
	type FeePayment = MockFeePayment<Self>;
	type IngressEgressFeeHandler = MockIngressEgressFeeHandler<AnyChain>;
	type BalanceApi = MockBalance;
	type CcmValidityChecker = AlwaysValid;
	type NetworkFee = NetworkFee;
}

pub const ALICE: <Test as frame_system::Config>::AccountId = 123u64;

cf_test_utilities::impl_test_helpers! {
	Test,
	RuntimeGenesisConfig::default(),
	|| {
		<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_broker(&ALICE).unwrap();
	},
}
