use crate::{LpBalanceApi, LpDepositHandler};
use cf_chains::assets::any::Asset;
use cf_primitives::AssetAmount;
use sp_runtime::{
	traits::{CheckedSub, Saturating},
	DispatchError, DispatchResult,
};

#[cfg(feature = "runtime-benchmarks")]
use cf_chains::ForeignChainAddress;

use super::{MockPallet, MockPalletStorage};

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

impl LpBalanceApi for MockBalance {
	type AccountId = u64;

	#[cfg(feature = "runtime-benchmarks")]
	fn register_liquidity_refund_address(_who: &Self::AccountId, _address: ForeignChainAddress) {}

	fn ensure_has_refund_address_for_pair(
		_who: &Self::AccountId,
		_base_asset: Asset,
		_quote_asset: Asset,
	) -> DispatchResult {
		Ok(())
	}

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
	fn asset_balances(who: &Self::AccountId) -> Vec<(Asset, AssetAmount)> {
		Asset::all()
			.map(|asset| {
				(asset, Self::get_storage(FREE_BALANCES, (who, asset)).unwrap_or_default())
			})
			.collect()
	}
}
