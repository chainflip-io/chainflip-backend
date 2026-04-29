use crate::*;
use frame_support::traits::UncheckedOnRuntimeUpgrade;

pub struct Migration<T: Config>(PhantomData<T>);

mod old {
	use super::*;
	use crate::general_lending::{InterestBreakdown, LiquidationStatus};
	use cf_traits::lending::LoanId;

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub struct GeneralLoan<T: Config> {
		pub id: LoanId,
		pub asset: Asset,
		pub created_at_block: BlockNumberFor<T>,
		pub owed_principal: AssetAmount,
		pub pending_interest: InterestBreakdown,
	}

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub struct LoanAccount<T: Config> {
		pub borrower_id: T::AccountId,
		pub collateral_topup_asset: Option<Asset>,
		// This field is being removed:
		pub collateral: BTreeMap<Asset, AssetAmount>,
		// Loans are modified: broker field is added
		pub loans: BTreeMap<LoanId, GeneralLoan<T>>,
		pub liquidation_status: LiquidationStatus,
		pub voluntary_liquidation_requested: bool,
	}

	#[frame_support::storage_alias]
	pub type LoanAccounts<T: Config> =
		StorageMap<Pallet<T>, Twox64Concat, <T as frame_system::Config>::AccountId, LoanAccount<T>>;
}

fn migrate_loan<T: Config>(old_loan: old::GeneralLoan<T>) -> general_lending::GeneralLoan<T> {
	general_lending::GeneralLoan {
		id: old_loan.id,
		asset: old_loan.asset,
		created_at_block: old_loan.created_at_block,
		owed_principal: old_loan.owed_principal,
		pending_interest: old_loan.pending_interest,
		broker: None,
	}
}

impl<T: Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		// Migrate user loan accounts: move collateral to supply pools, drop the `collateral`
		// field, and add `broker: None` to every nested loan.
		let old_accounts: Vec<_> = old::LoanAccounts::<T>::drain().collect();

		for (account_id, old_account) in &old_accounts {
			// Supply each collateral asset into the corresponding lending pool.
			for (asset, amount) in &old_account.collateral {
				if *amount == 0 {
					continue;
				}

				// Ensure the lending pool exists, creating it if needed.
				if !GeneralLendingPools::<T>::contains_key(asset) {
					if let Err(e) = Pallet::<T>::new_lending_pool(*asset) {
						log::error!(
							"Failed to create lending pool for {:?} during migration: {:?}",
							asset,
							e
						);
						continue;
					}
				}

				if let Err(e) = general_lending::supply_funds::<T>(
					account_id.clone(),
					*asset,
					*amount,
					SupplyAddedActionType::Manual,
				) {
					log::error!(
						"Failed to supply collateral for {:?} asset {:?} amount {}: {:?}",
						account_id,
						asset,
						amount,
						e
					);
				}
			}

			// Write the new LoanAccount without the collateral field, with `broker: None`
			// added to each loan.
			let migrated_loans = old_account
				.loans
				.iter()
				.map(|(id, loan)| (*id, migrate_loan::<T>(loan.clone())))
				.collect();

			LoanAccounts::<T>::insert(
				account_id,
				LoanAccount {
					borrower_id: old_account.borrower_id.clone(),
					collateral_topup_asset: old_account.collateral_topup_asset,
					loans: migrated_loans,
					liquidation_status: old_account.liquidation_status.clone(),
					voluntary_liquidation_requested: old_account.voluntary_liquidation_requested,
				},
			);
		}

		log::info!(
			"Migrated {} loan accounts (collateral moved to supply pools, broker field added)",
			old_accounts.len(),
		);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let account_count = old::LoanAccounts::<T>::iter().count() as u32;

		let mut total_borrowed: BTreeMap<Asset, AssetAmount> = BTreeMap::new();
		let mut total_collateral: BTreeMap<Asset, AssetAmount> = BTreeMap::new();
		for (_, account) in old::LoanAccounts::<T>::iter() {
			for loan in account.loans.values() {
				*total_borrowed.entry(loan.asset).or_default() += loan.owed_principal;
			}
			for (asset, amount) in &account.collateral {
				*total_collateral.entry(*asset).or_default() += amount;
			}
		}

		let mut total_supplied: BTreeMap<Asset, AssetAmount> = BTreeMap::new();
		for (asset, pool) in crate::GeneralLendingPools::<T>::iter() {
			for LendingSupplyPosition { lp_id: _, total_amount } in pool.get_all_supply_positions()
			{
				*total_supplied.entry(asset).or_default() += total_amount;
			}
		}

		log::info!("Total collateral before: {:?}", total_collateral);
		log::info!("Total supplied before: {:?}", total_supplied);

		Ok((account_count, total_borrowed, total_collateral, total_supplied).encode())
	}

	#[cfg(feature = "try-runtime")]
	#[expect(clippy::type_complexity)]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let (old_account_count, old_total_borrowed, total_collateral, old_total_supplied): (
			u32,
			BTreeMap<Asset, AssetAmount>,
			BTreeMap<Asset, AssetAmount>,
			BTreeMap<Asset, AssetAmount>,
		) = Decode::decode(&mut &state[..]).expect("pre_upgrade encoded state");

		let new_account_count = LoanAccounts::<T>::iter().count() as u32;
		ensure!(old_account_count == new_account_count, "Loan account count mismatch");

		// Verify total borrowed amounts are unchanged.
		let mut new_total_borrowed: BTreeMap<Asset, AssetAmount> = BTreeMap::new();
		for (_, account) in LoanAccounts::<T>::iter() {
			for loan in account.loans.values() {
				*new_total_borrowed.entry(loan.asset).or_default() += loan.owed_principal;
			}
		}
		ensure!(
			old_total_borrowed == new_total_borrowed,
			"Total borrowed amounts changed during migration"
		);

		// Every loan should have `broker: None` after the migration.
		ensure!(
			LoanAccounts::<T>::iter()
				.all(|(_, account)| account.loans.values().all(|loan| loan.broker.is_none())),
			"User loans should have broker = None after migration",
		);

		// Verify that total supplied increased by exactly the old collateral amounts.
		let mut new_total_supplied: BTreeMap<Asset, AssetAmount> = BTreeMap::new();
		for (asset, pool) in crate::GeneralLendingPools::<T>::iter() {
			for LendingSupplyPosition { lp_id: _, total_amount } in pool.get_all_supply_positions()
			{
				*new_total_supplied.entry(asset).or_default() += total_amount;
			}
		}

		// For each asset, new_supplied == old_supplied + collateral that was migrated.
		let all_assets: BTreeSet<Asset> = old_total_supplied
			.keys()
			.chain(total_collateral.keys())
			.chain(new_total_supplied.keys())
			.copied()
			.collect();

		for asset in all_assets {
			let old_supplied = old_total_supplied.get(&asset).copied().unwrap_or(0);
			let collateral = total_collateral.get(&asset).copied().unwrap_or(0);
			let new_supplied = new_total_supplied.get(&asset).copied().unwrap_or(0);
			let expected = old_supplied + collateral;

			log::info!(
				"Asset {:?}: supplied before={}, collateral={}, supplied after={}, expected={}",
				asset,
				old_supplied,
				collateral,
				new_supplied,
				expected
			);
			ensure!(
				new_supplied == expected,
				"Supply mismatch: expected total supplied to increase by collateral amount"
			);
		}

		Ok(())
	}
}
