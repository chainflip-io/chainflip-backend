use crate::{BalanceApi, LpDepositHandler};
use cf_chains::assets::any::{Asset, AssetMap};
use cf_primitives::{AssetAmount, BalancesInfo};
use frame_support::sp_runtime::{
	traits::{CheckedSub, Saturating},
	DispatchError, DispatchResult,
};

#[cfg(feature = "runtime-benchmarks")]
use cf_chains::ForeignChainAddress;

use super::{MockPallet, MockPalletStorage};

use crate::LpApi;

pub struct MockBalance;

impl LpDepositHandler for MockBalance {
	type AccountId = u64;

	fn add_deposit(
		who: &Self::AccountId,
		asset: Asset,
		amount: AssetAmount,
	) -> frame_support::pallet_prelude::DispatchResult {
		Self::try_credit_account(who, asset, amount)
	}
}

impl MockPallet for MockBalance {
	const PREFIX: &'static [u8] = b"LP_BALANCE";
}

const FREE_BALANCES: &[u8] = b"FREE_BALANCES";

impl BalanceApi for MockBalance {
	type AccountId = u64;

	fn try_credit_account(
		who: &Self::AccountId,
		asset: Asset,
		amount_to_credit: AssetAmount,
	) -> DispatchResult {
		Self::mutate_storage::<(u64, cf_primitives::Asset), _, _, _, _>(
			FREE_BALANCES,
			&(*who, asset),
			|amount| {
				let amount = amount.get_or_insert_with(|| 0);
				*amount = amount.saturating_add(amount_to_credit);
			},
		);
		Ok(())
	}

	fn try_debit_account(
		who: &Self::AccountId,
		asset: Asset,
		amount_to_debit: AssetAmount,
	) -> DispatchResult {
		Self::mutate_storage::<(u64, cf_primitives::Asset), _, _, _, _>(
			FREE_BALANCES,
			&(*who, asset),
			|amount| {
				let amount = amount.get_or_insert_with(|| 0);

				*amount = amount
					.checked_sub(&amount_to_debit)
					.ok_or::<DispatchError>("Insufficient balance".into())?;
				Ok(())
			},
		)
	}

	fn record_fees(_who: &Self::AccountId, _amount: AssetAmount, _asset: Asset) {}

	fn free_balances(who: &Self::AccountId) -> Result<AssetMap<AssetAmount>, DispatchError> {
		Ok(AssetMap::try_from_iter(Asset::all().map(|asset| {
			(asset, Self::get_storage(FREE_BALANCES, (who, asset)).unwrap_or_default())
		}))
		.unwrap())
	}

	fn collected_rejected_funds(_asset: Asset, _amount: AssetAmount) {
		unimplemented!()
	}

	fn kill_balance(_who: &Self::AccountId) {
		unimplemented!()
	}

	#[cfg(feature = "try-runtime")]
	fn get_balances_info() -> BalancesInfo {
		unimplemented!()
	}
}

pub struct MockLpApi;

impl LpApi for MockLpApi {
	type AccountId = u64;

	#[cfg(feature = "runtime-benchmarks")]
	fn register_liquidity_refund_address(_: &Self::AccountId, _: ForeignChainAddress) {}

	fn ensure_has_refund_address_for_pair(
		_who: &Self::AccountId,
		_base_asset: Asset,
		_quote_asset: Asset,
	) -> DispatchResult {
		Ok(())
	}
}
