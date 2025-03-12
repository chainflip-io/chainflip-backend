use crate::{BalanceApi, LpDepositHandler};
use cf_chains::assets::any::{Asset, AssetMap};
use cf_primitives::AssetAmount;
use frame_support::sp_runtime::{
	traits::{CheckedSub, Saturating},
	DispatchError, DispatchResult,
};

use super::{MockPallet, MockPalletStorage};

use crate::LpRegistration;

pub struct MockBalance;

type AccountId = u64;

impl LpDepositHandler for MockBalance {
	type AccountId = AccountId;

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
	type AccountId = AccountId;

	fn credit_account(who: &Self::AccountId, asset: Asset, amount_to_credit: AssetAmount) {
		Self::mutate_storage::<(AccountId, cf_primitives::Asset), _, _, _, _>(
			FREE_BALANCES,
			&(*who, asset),
			|amount| {
				let amount = amount.get_or_insert_with(|| 0);
				*amount = amount.saturating_add(amount_to_credit);
			},
		);
	}

	fn try_credit_account(
		who: &Self::AccountId,
		asset: Asset,
		amount_to_credit: AssetAmount,
	) -> DispatchResult {
		Self::credit_account(who, asset, amount_to_credit);
		Ok(())
	}

	fn try_debit_account(
		who: &Self::AccountId,
		asset: Asset,
		amount_to_debit: AssetAmount,
	) -> DispatchResult {
		Self::mutate_storage::<(AccountId, cf_primitives::Asset), _, _, _, _>(
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

	fn free_balances(who: &Self::AccountId) -> AssetMap<AssetAmount> {
		AssetMap::from_iter_or_default(Asset::all().map(|asset| {
			(asset, Self::get_storage(FREE_BALANCES, (who, asset)).unwrap_or_default())
		}))
	}

	fn get_balance(who: &Self::AccountId, asset: Asset) -> AssetAmount {
		Self::get_storage(FREE_BALANCES, (who, asset)).unwrap_or_default()
	}
}

pub struct MockLpRegistration;

impl MockPallet for MockLpRegistration {
	const PREFIX: &'static [u8] = b"LP_REGISTRATION";
}

const REFUND_ADDRESS_REGISTRATION: &[u8] = b"IS_REGISTERED_FOR_ASSET";

impl MockLpRegistration {
	pub fn register_refund_address(account_id: AccountId, asset: Asset) {
		Self::mutate_storage::<(AccountId, Asset), _, _, (), _>(
			REFUND_ADDRESS_REGISTRATION,
			&(account_id, asset),
			|is_registered: &mut Option<()>| {
				*is_registered = Some(());
			},
		);
	}
}

impl LpRegistration for MockLpRegistration {
	type AccountId = u64;

	#[cfg(feature = "runtime-benchmarks")]
	fn register_liquidity_refund_address(_: &Self::AccountId, _: cf_chains::ForeignChainAddress) {}

	fn ensure_has_refund_address_for_asset(who: &Self::AccountId, asset: Asset) -> DispatchResult {
		if Self::get_storage::<(AccountId, Asset), ()>(REFUND_ADDRESS_REGISTRATION, (*who, asset))
			.is_some()
		{
			Ok(())
		} else {
			Err(DispatchError::Other("no refund address"))
		}
	}
}
