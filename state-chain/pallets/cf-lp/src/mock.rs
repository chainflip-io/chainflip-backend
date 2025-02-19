use crate as pallet_cf_lp;
use crate::PalletSafeMode;
use cf_chains::{
	address::{AddressDerivationApi, AddressDerivationError},
	assets::any::Asset,
	AnyChain, Chain, Ethereum,
};
use cf_primitives::{chains::assets, AssetAmount, ChannelId};
#[cfg(feature = "runtime-benchmarks")]
use cf_traits::mocks::fee_payment::MockFeePayment;
use cf_traits::{
	impl_mock_chainflip, impl_mock_runtime_safe_mode,
	mocks::{
		address_converter::MockAddressConverter, deposit_handler::MockDepositHandler,
		egress_handler::MockEgressHandler, swap_request_api::MockSwapRequestHandler,
	},
	AccountRoleRegistry, BalanceApi, BoostApi, HistoricalFeeMigration,
};
use frame_support::{
	assert_ok, derive_impl, parameter_types, sp_runtime::app_crypto::sp_core::H160,
};
use frame_system as system;
use sp_runtime::{traits::IdentityLookup, Permill};
use std::{cell::RefCell, collections::BTreeMap};

use sp_std::str::FromStr;

type AccountId = u64;

pub struct MockAddressDerivation;

impl AddressDerivationApi<Ethereum> for MockAddressDerivation {
	fn generate_address(
		_source_asset: assets::eth::Asset,
		_channel_id: ChannelId,
	) -> Result<<Ethereum as Chain>::ChainAccount, AddressDerivationError> {
		Ok(H160::from_str("F29aB9EbDb481BE48b80699758e6e9a3DBD609C6").unwrap())
	}

	fn generate_address_and_state(
		source_asset: <Ethereum as Chain>::ChainAsset,
		channel_id: ChannelId,
	) -> Result<
		(<Ethereum as Chain>::ChainAccount, <Ethereum as Chain>::DepositChannelState),
		AddressDerivationError,
	> {
		Ok((Self::generate_address(source_asset, channel_id)?, Default::default()))
	}
}
type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		LiquidityProvider: pallet_cf_lp,
	}
);

thread_local! {
	pub static BALANCE_MAP: RefCell<BTreeMap<AccountId, AssetAmount>> = RefCell::new(BTreeMap::new());
}

pub struct MockMigrationHelper;

impl HistoricalFeeMigration for MockMigrationHelper {
	type AccountId = AccountId;

	fn migrate_historical_fee(_account_id: Self::AccountId, _asset: Asset, _amount: AssetAmount) {
		todo!()
	}

	fn get_fee_amount(_account_id: Self::AccountId, _asset: Asset) -> AssetAmount {
		todo!()
	}
}

pub struct MockBalanceApi;

impl BalanceApi for MockBalanceApi {
	type AccountId = AccountId;

	fn credit_account(who: &Self::AccountId, _asset: Asset, amount: AssetAmount) {
		BALANCE_MAP.with(|balance_map| {
			let mut balance_map = balance_map.borrow_mut();
			*balance_map.entry(*who).or_default() += amount;
		});
	}

	fn try_credit_account(
		who: &Self::AccountId,
		asset: cf_primitives::Asset,
		amount: cf_primitives::AssetAmount,
	) -> frame_support::dispatch::DispatchResult {
		Self::credit_account(who, asset, amount);
		Ok(())
	}

	fn try_debit_account(
		who: &Self::AccountId,
		_asset: cf_primitives::Asset,
		amount: cf_primitives::AssetAmount,
	) -> frame_support::dispatch::DispatchResult {
		BALANCE_MAP.with(|balance_map| {
			let mut balance_map = balance_map.borrow_mut();
			let balance = balance_map.entry(who.to_owned()).or_default();
			*balance = balance.checked_sub(amount).ok_or("Insufficient balance")?;
			Ok(())
		})
	}

	fn free_balances(who: &Self::AccountId) -> assets::any::AssetMap<cf_primitives::AssetAmount> {
		BALANCE_MAP.with(|balance_map| {
			assets::any::AssetMap::from_iter_or_default(
				Asset::all().map(|asset| {
					(asset, balance_map.borrow().get(who).cloned().unwrap_or_default())
				}),
			)
		})
	}

	fn get_balance(_who: &Self::AccountId, _asset: Asset) -> AssetAmount {
		todo!()
	}
}

impl MockBalanceApi {
	pub fn insert_balance(account: AccountId, amount: AssetAmount) {
		BALANCE_MAP.with(|balance_map| {
			balance_map.borrow_mut().insert(account, amount);
		});
	}

	pub fn get_balance(account: &AccountId) -> Option<AssetAmount> {
		BALANCE_MAP.with(|balance_map| balance_map.borrow().get(account).cloned())
	}
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl system::Config for Test {
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
}

impl_mock_chainflip!(Test);

parameter_types! {
	pub const NetworkFee: Permill = Permill::from_percent(0);
	pub static BoostBalance: AssetAmount = Default::default();
}

impl_mock_runtime_safe_mode!(liquidity_provider: PalletSafeMode);
impl crate::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type DepositHandler = MockDepositHandler<AnyChain, Self>;
	type EgressHandler = MockEgressHandler<AnyChain>;
	type AddressConverter = MockAddressConverter;
	type SafeMode = MockRuntimeSafeMode;
	type WeightInfo = ();
	type PoolApi = Self;
	type BalanceApi = MockBalanceApi;
	#[cfg(feature = "runtime-benchmarks")]
	type FeePayment = MockFeePayment<Self>;
	type BoostApi = MockIngressEgressBoostApi;
	type SwapRequestHandler = MockSwapRequestHandler<(Ethereum, MockEgressHandler<Ethereum>)>;
	type MigrationHelper = MockMigrationHelper;
}

pub struct MockIngressEgressBoostApi;
impl BoostApi for MockIngressEgressBoostApi {
	type AccountId = AccountId;
	type AssetMap = cf_chains::assets::any::AssetMap<AssetAmount>;

	fn boost_pool_account_balances(_who: &Self::AccountId) -> Self::AssetMap {
		Self::AssetMap::from_fn(|_| BoostBalance::get())
	}
}

impl MockIngressEgressBoostApi {
	pub fn set_boost_funds(amount: AssetAmount) -> Result<(), ()> {
		BoostBalance::set(amount);
		Ok(())
	}
	pub fn remove_boost_funds(amount: AssetAmount) -> Result<(), ()> {
		if amount > BoostBalance::get() {
			return Err(());
		}
		BoostBalance::set(amount - BoostBalance::get());
		Ok(())
	}
}

pub const LP_ACCOUNT: AccountId = 1;
pub const LP_ACCOUNT_2: AccountId = 3;
pub const NON_LP_ACCOUNT: AccountId = 2;

cf_test_utilities::impl_test_helpers! {
	Test,
	RuntimeGenesisConfig::default(),
	|| {
		assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_liquidity_provider(
			&LP_ACCOUNT,
		));
		assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_liquidity_provider(
			&LP_ACCOUNT_2,
		));
		assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_validator(
			&NON_LP_ACCOUNT,
		));
	}
}
