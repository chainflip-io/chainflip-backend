use cf_traits::lending::BoostFinalisationOutcome;

use crate::{
	core_lending_pool::{CoreLendingPool, CoreLoanId},
	general_lending::{create_new_loan, fund_loan},
};

use super::*;

pub const BOOST_FEE: BasisPoints = 5;

#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct BoostPool {
	// Fee charged by the pool
	pub fee_bps: BasisPoints,
	pub core_pool_id: CorePoolId,
}

#[derive(Encode, Decode, DecodeWithMemTracking, TypeInfo, Debug, PartialEq, Eq, Clone)]
pub struct BoostPoolContribution {
	pub core_pool_id: CorePoolId,
	pub loan_id: CoreLoanId,
	pub boosted_amount: AssetAmount,
	pub network_fee: AssetAmount,
}

/// Represents a deposit that was boosted and now awaits finalisation
#[derive(Encode, Decode, DecodeWithMemTracking, TypeInfo, Debug, PartialEq, Eq, Clone)]
pub struct BoostedDeposit {
	/// Full deposit amount. We expect to receive this much when deposit is finalised.
	pub deposit_amount: AssetAmount,
	/// Loan from the general lending pool, if it contributed to the boost.
	pub lending_loan_id: Option<LoanId>,
	/// Boost pool's contribution in case it was used for this deposit.
	pub boost_pool_contribution: Option<BoostPoolContribution>,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct BoostPoolId {
	pub asset: Asset,
	pub tier: BoostPoolTier,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct OwedAmount<AmountT> {
	pub total: AmountT,
	pub fee: AmountT,
}

#[derive(Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub struct BoostPoolDetails<AccountId> {
	pub available_amounts: BTreeMap<AccountId, AssetAmount>,
	pub pending_boosts:
		BTreeMap<PrewitnessedDepositId, BTreeMap<AccountId, OwedAmount<AssetAmount>>>,
	pub pending_withdrawals: BTreeMap<AccountId, BTreeSet<PrewitnessedDepositId>>,
	pub network_fee_deduction_percent: Percent,
}

/// Splits boost fee into two amounts (network fee, pool fee) according to boost config
fn split_between_network_and_pool<T: Config>(fee: AssetAmount) -> (AssetAmount, AssetAmount) {
	let network_fee_portion = BoostConfig::<T>::get().network_fee_deduction_from_boost_percent;
	let network_fee = network_fee_portion * fee;
	let pool_fee = fee.saturating_sub(network_fee);

	(network_fee, pool_fee)
}

pub fn boost_pools_iter<T: Config>(
) -> impl Iterator<Item = (Asset, BoostPoolTier, CoreLendingPool<T::AccountId>)> {
	BoostPools::<T>::iter().filter_map(move |(asset, tier, pool)| {
		CorePools::<T>::get(asset, pool.core_pool_id).map(|core_pool| (asset, tier, core_pool))
	})
}

fn boost_pools_for_asset_iter<T: Config>(
	asset: Asset,
) -> impl Iterator<Item = (BoostPoolTier, CoreLendingPool<T::AccountId>)> {
	BoostPools::<T>::iter_prefix(asset).filter_map(move |(tier, pool)| {
		CorePools::<T>::get(asset, pool.core_pool_id).map(|core_pool| (tier, core_pool))
	})
}

pub fn get_boost_pool_details<T: Config>(
	asset: Asset,
) -> BTreeMap<BoostPoolTier, BoostPoolDetails<T::AccountId>> {
	let network_fee_deduction_percent =
		BoostConfig::<T>::get().network_fee_deduction_from_boost_percent;

	boost_pools_for_asset_iter::<T>(asset)
		.map(|(tier, core_pool)| {
			let pending_boosts = core_pool
				.get_pending_loans()
				.values()
				.map(|loan| {
					let LoanUsage::Boost(deposit_id) = loan.usage;
					(deposit_id, loan)
				})
				.map(|(deposit_id, loan)| {
					let Some(BoostPoolContribution { boosted_amount, network_fee, .. }) =
						BoostedDeposits::<T>::get(asset, deposit_id)
							.and_then(|d| d.boost_pool_contribution)
					else {
						return (deposit_id, BTreeMap::default());
					};

					let total_owed_amount = boosted_amount.saturating_sub(network_fee);

					let boosters_fee =
						fee_from_boosted_amount(boosted_amount, tier).saturating_sub(network_fee);

					let owed_amounts = loan
						.shares
						.iter()
						.map(|(acc_id, share)| {
							(
								acc_id.clone(),
								OwedAmount {
									total: *share * total_owed_amount,
									fee: *share * boosters_fee,
								},
							)
						})
						.collect();

					(deposit_id, owed_amounts)
				})
				.collect();

			let pending_withdrawals = core_pool
				.pending_withdrawals
				.iter()
				.map(|(acc_id, loan_ids)| {
					let deposit_ids = loan_ids
						.iter()
						.filter_map(|loan_id| {
							core_pool.pending_loans.get(loan_id).map(|loan| {
								let LoanUsage::Boost(deposit_id) = loan.usage;
								deposit_id
							})
						})
						.collect();

					(acc_id.clone(), deposit_ids)
				})
				.collect();
			(
				tier,
				BoostPoolDetails {
					available_amounts: core_pool.get_amounts(),
					pending_boosts,
					pending_withdrawals,
					network_fee_deduction_percent,
				},
			)
		})
		.collect()
}

impl<T: Config> BoostApi for Pallet<T> {
	#[transactional]
	fn try_boosting(
		deposit_id: PrewitnessedDepositId,
		asset: Asset,
		deposit_amount: AssetAmount,
		max_boost_fee_bps: BasisPoints,
	) -> Result<BoostOutcome, DispatchError> {
		ensure!(BOOST_FEE <= max_boost_fee_bps, "max boost fee violation");

		// Derive the total fee and the amount that needs to be funded:
		let total_fee = fee_from_boosted_amount(deposit_amount, BOOST_FEE);
		let required_amount = deposit_amount.saturating_sub(total_fee);

		// Check available liquidity from both sources:
		let lending_available =
			GeneralLendingPools::<T>::get(asset).map_or(0, |p| p.available_amount);
		let boost_available = BoostPools::<T>::get(asset, BOOST_FEE)
			.and_then(|pool| CorePools::<T>::get(asset, pool.core_pool_id))
			.map_or(0, |p| p.available_amount.into_asset_amount());

		ensure!(
			lending_available.saturating_add(boost_available) >= required_amount,
			Error::<T>::InsufficientBoostLiquidity
		);

		// Lending pool has priority: use it up to its available capacity.
		let lending_pool_principal = lending_available.min(required_amount);
		let boost_pool_principal = required_amount.saturating_sub(lending_pool_principal);

		let lending_pool_fee =
			Permill::from_rational(lending_pool_principal, required_amount) * total_fee;

		let boost_pool_fee = total_fee.saturating_sub(lending_pool_fee);

		// Allocate from the lending pool (if possible):
		let lending_loan_id = if lending_pool_principal > 0 {
			let (network_fee, pool_fee) = split_between_network_and_pool::<T>(lending_pool_fee);

			let mut loan = create_new_loan::<T>(asset);
			let loan_id = loan.id;

			Self::deposit_event(Event::LoanCreated {
				loan_id,
				loan_type: LoanType::Boost(deposit_id),
				asset,
				principal_amount: lending_pool_principal,
			});

			fund_loan::<T>(&mut loan, lending_pool_principal, pool_fee, network_fee)?;
			BoostLoans::<T>::insert(loan_id, loan);
			Some(loan_id)
		} else {
			None
		};

		// Allocate from the boost pool (if the lending pool couldn't cover everything):
		let boost_pool_contribution = if boost_pool_principal > 0 {
			let boost_pool =
				BoostPools::<T>::get(asset, BOOST_FEE).ok_or(Error::<T>::PoolDoesNotExist)?;

			let core_loan_id =
				CorePools::<T>::try_mutate(asset, boost_pool.core_pool_id, |maybe_pool| {
					let pool = maybe_pool.as_mut().ok_or(Error::<T>::PoolDoesNotExist)?;

					pool.new_loan(boost_pool_principal, LoanUsage::Boost(deposit_id))
						.map_err(|_| Error::<T>::InsufficientBoostLiquidity)
				})?;

			let (network_fee, _pool_fee) = split_between_network_and_pool::<T>(boost_pool_fee);

			Some(BoostPoolContribution {
				core_pool_id: boost_pool.core_pool_id,
				loan_id: core_loan_id,
				boosted_amount: boost_pool_principal.saturating_add(boost_pool_fee),
				network_fee,
			})
		} else {
			None
		};

		BoostedDeposits::<T>::insert(
			asset,
			deposit_id,
			BoostedDeposit { deposit_amount, lending_loan_id, boost_pool_contribution },
		);

		let mut amounts = BTreeMap::new();
		if lending_pool_principal > 0 {
			amounts.insert(BoostSource::LendingPool, lending_pool_principal + lending_pool_fee);
		}
		if boost_pool_principal > 0 {
			amounts.insert(BoostSource::BoostPool, boost_pool_principal + boost_pool_fee);
		}

		Ok(BoostOutcome { total_fee, amounts })
	}

	fn finalise_boost(deposit_id: PrewitnessedDepositId, asset: Asset) -> BoostFinalisationOutcome {
		let Some(BoostedDeposit { deposit_amount, lending_loan_id, boost_pool_contribution }) =
			BoostedDeposits::<T>::take(asset, deposit_id)
		else {
			log_or_panic!("Boost record for a finalised deposit not found: {}", deposit_id);
			return Default::default();
		};

		// Settle boost pool loan (if any):
		let network_fee_from_legacy_pool = if let Some(BoostPoolContribution {
			core_pool_id,
			loan_id,
			boosted_amount,
			network_fee,
		}) = &boost_pool_contribution
		{
			CorePools::<T>::mutate(asset, core_pool_id, |maybe_pool| {
				let Some(pool) = maybe_pool.as_mut() else {
					log_or_panic!(
						"Core pool not found for boost pool on finalisation (asset: {:?})",
						asset
					);
					return;
				};

				for (booster_id, unlocked_amount) in
					pool.make_repayment(*loan_id, boosted_amount.saturating_sub(*network_fee))
				{
					T::Balance::credit_account(&booster_id, asset, unlocked_amount);
				}

				pool.finalise_loan(*loan_id);
			});
			*network_fee
		} else {
			0
		};

		// Settle lending pool loan (if any):
		if let Some(loan_id) = lending_loan_id {
			if let Some(mut loan) = BoostLoans::<T>::take(loan_id) {
				// The lending pool is repaid with the deposit amount minus the boost pool's
				// boosted amount (principal + fee), since that goes back to the boost pool.
				let boost_pool_total = boost_pool_contribution.map_or(0, |c| c.boosted_amount);
				let lending_repayment = deposit_amount.saturating_sub(boost_pool_total);

				loan.repay_principal(lending_repayment, LoanRepaidActionType::BoostFinalisation);

				if loan.owed_principal > 0 {
					log_or_panic!(
						"Boost loan is not fully repaid on finalisation (loan_id: {:?})",
						loan_id
					);
				}

				loan.settle(false /* via liquidation */);
			} else {
				log_or_panic!("Boost loan not found for (loan_id: {:?})", loan_id);
			}
		}

		// Only legacy portion of the network fee is returned here (the lending pool's portion has
		// already been credited to the network at boost time).
		BoostFinalisationOutcome { network_fee: network_fee_from_legacy_pool }
	}

	fn process_deposit_as_lost(deposit_id: PrewitnessedDepositId, asset: Asset) {
		let Some(BoostedDeposit { lending_loan_id, boost_pool_contribution, .. }) =
			BoostedDeposits::<T>::take(asset, deposit_id)
		else {
			log_or_panic!("Boost record for a lost deposit not found: {}", deposit_id);
			return;
		};

		// Boost pool absorbs the loss (loan finalised without repayment):
		if let Some(contribution) = boost_pool_contribution {
			CorePools::<T>::mutate(asset, contribution.core_pool_id, |maybe_pool| {
				let Some(pool) = maybe_pool.as_mut() else {
					log_or_panic!(
						"Core pool not found for boost pool on loss (asset: {:?})",
						asset
					);
					return;
				};

				pool.finalise_loan(contribution.loan_id);
			});
		}

		// Lending pool settles its loan (loss socialised across lenders):
		if let Some(loan_id) = lending_loan_id {
			if let Some(loan) = BoostLoans::<T>::take(loan_id) {
				loan.settle(false /* via_liquidation */);
			} else {
				log_or_panic!("Boost loan not found for (loan_id: {:?})", loan_id);
			}
		}
	}
}

impl<T: Config> Pallet<T> {
	pub fn boost_pool_account_balance(who: &T::AccountId, asset: Asset) -> AssetAmount {
		let available = BoostPools::<T>::iter_prefix(asset).fold(0, |acc, (_tier, pool)| {
			let Some(core_pool) = CorePools::<T>::get(asset, pool.core_pool_id) else {
				return 0;
			};

			acc + core_pool.get_available_amount_for_account(who).unwrap_or(0)
		});

		let in_all_boosted_deposits =
			BoostedDeposits::<T>::iter_prefix(asset).fold(0, |acc, (_, deposit)| {
				let Some(BoostPoolContribution {
					core_pool_id,
					loan_id,
					boosted_amount,
					network_fee,
				}) = deposit.boost_pool_contribution
				else {
					return acc;
				};

				let Some(core_pool) = CorePools::<T>::get(asset, core_pool_id) else {
					return acc;
				};

				let Some(loan) = core_pool.pending_loans.get(&loan_id) else { return acc };

				let Some(share) = loan.shares.get(who) else { return acc };

				acc + *share * boosted_amount.saturating_sub(network_fee)
			});

		available + in_all_boosted_deposits
	}
}

/// Boosted amount is the amount provided by the pool plus boost fee,
/// (and the sum of all boosted amounts from each participating pool
/// must be equal the deposit amount being boosted). The fee is payed
/// per boosted amount, and so here we multiply by fee_bps directly.
fn fee_from_boosted_amount(amount_to_boost: AssetAmount, fee_bps: u16) -> AssetAmount {
	use cf_primitives::BASIS_POINTS_PER_MILLION;
	let fee_permill = Permill::from_parts(fee_bps as u32 * BASIS_POINTS_PER_MILLION);

	fee_permill * amount_to_boost
}

#[test]
fn test_fee_math() {
	assert_eq!(fee_from_boosted_amount(1_000_000, 10), 1_000);
}
