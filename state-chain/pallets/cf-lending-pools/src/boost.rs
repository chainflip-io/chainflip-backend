use cf_traits::lending::BoostFinalisationOutcome;

use crate::{
	core_lending_pool::{CoreLendingPool, CoreLoanId},
	general_lending::{check_pool_caps_after_borrow, create_new_loan, fund_loan, OraclePriceCache},
};
use frame_support::sp_runtime::{helpers_128bit::multiply_by_rational_with_rounding, Rounding};
use sp_std::collections::btree_set::BTreeSet;

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

		// Two-step split: first allocate proportionally using the lending pool's *full*
		// available balance (so we don't bias toward the legacy boost pool when utilisation
		// is healthy), then cap the lending pool's contribution to leave room for the
		// network portion of the origination fee. The cap only kicks in when the
		// proportional allocation would push the lending pool past what `fund_loan` can
		// accept (i.e. near 100% utilisation) — otherwise the split is unchanged. Any
		// overflow at the cap shifts to the boost pool.
		let (lending_pool_principal, boost_pool_principal) = {
			let (lending_pool_principal, boost_pool_principal) = try_split_required_amount(
				required_amount,
				lending_available,
				boost_available,
				BoostConfig::<T>::get().min_lending_pool_share,
			)
			.map_err(Error::<T>::from)?;

			cap_lending_principal_for_fee::<T>(
				lending_pool_principal,
				boost_pool_principal,
				lending_available,
				boost_available,
				required_amount,
				total_fee,
			)?
		};

		let lending_pool_fee =
			Permill::from_rational(lending_pool_principal, required_amount) * total_fee;

		let boost_pool_fee = total_fee.saturating_sub(lending_pool_fee);

		// Allocate from the lending pool (if possible):
		let lending_loan_id = if lending_pool_principal > 0 {
			let (network_fee, pool_fee) = split_between_network_and_pool::<T>(lending_pool_fee);

			let mut loan = create_new_loan::<T>(asset, None);
			let loan_id = loan.id;

			Self::deposit_event(Event::LoanCreated {
				loan_id,
				loan_type: LoanType::Boost(deposit_id),
				asset,
				principal_amount: lending_pool_principal,
				broker: None,
			});

			fund_loan::<T>(&mut loan, lending_pool_principal, pool_fee, network_fee)?;

			BoostLoans::<T>::insert(loan_id, loan);

			// Boost loans are not collateralised, so only the loan-asset pool is affected.
			check_pool_caps_after_borrow::<T>(asset, &OraclePriceCache::<T>::default())?;

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
		let mut fees = BTreeMap::new();
		if lending_pool_principal > 0 {
			amounts.insert(BoostSource::LendingPool, lending_pool_principal + lending_pool_fee);
			fees.insert(BoostSource::LendingPool, lending_pool_fee);
		}
		if boost_pool_principal > 0 {
			amounts.insert(BoostSource::BoostPool, boost_pool_principal + boost_pool_fee);
			fees.insert(BoostSource::BoostPool, boost_pool_fee);
		}

		Ok(BoostOutcome { amounts, fees })
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

#[derive(Debug)]
struct SplitRequiredAmountError;

impl<T: Config> From<SplitRequiredAmountError> for Error<T> {
	fn from(_: SplitRequiredAmountError) -> Self {
		Error::<T>::InsufficientBoostLiquidity
	}
}

/// Largest principal the lending pool may fund without `fund_loan` rejecting the loan
/// for failing to cover the network portion of the origination fee from `available_amount`.
///
/// For principal `p`, the network portion is `network_fee_full * p/required`, where
/// `network_fee_full = split_between_network_and_pool(total_fee).0` is the network portion
/// if the lending pool covered all of `required_amount`. `fund_loan` enforces
/// `p + (p/required) * network_fee_full <= lending_available`, which solves to
/// `p <= lending_available * required / (required + network_fee_full)`.
fn lending_principal_cap_for_fee<T: Config>(
	lending_available: AssetAmount,
	total_fee: AssetAmount,
	required_amount: AssetAmount,
) -> AssetAmount {
	if required_amount == 0 {
		return lending_available;
	}
	let (network_fee_full, _) = split_between_network_and_pool::<T>(total_fee);
	// Note: we use `multiply_by_rational_with_rounding` rather than the `Permill::from_rational`
	// pattern used elsewhere for fee calculations because the cap is a hard liquidity
	// constraint — `Permill`'s 1e-6 precision loss can shrink the cap enough to push the
	// boost pool past its available balance when buffers are tight.
	multiply_by_rational_with_rounding(
		lending_available,
		required_amount,
		required_amount.saturating_add(network_fee_full),
		Rounding::Down,
	)
	.unwrap_or(0)
}

/// If the proportional split would push the lending pool past the cap that leaves room for
/// the origination network fee, shift the overflow to the boost pool. Fails if the boost
/// pool can't absorb it.
fn cap_lending_principal_for_fee<T: Config>(
	lending_pool_principal: AssetAmount,
	boost_pool_principal: AssetAmount,
	lending_available: AssetAmount,
	boost_available: AssetAmount,
	required_amount: AssetAmount,
	total_fee: AssetAmount,
) -> Result<(AssetAmount, AssetAmount), DispatchError> {
	let cap = lending_principal_cap_for_fee::<T>(lending_available, total_fee, required_amount);
	if lending_pool_principal <= cap {
		return Ok((lending_pool_principal, boost_pool_principal));
	}
	let overflow = lending_pool_principal.saturating_sub(cap);
	let new_boost_pool_principal = boost_pool_principal.saturating_add(overflow);
	ensure!(new_boost_pool_principal <= boost_available, Error::<T>::InsufficientBoostLiquidity);
	Ok((cap, new_boost_pool_principal))
}

/// Split `required_amount` between the lending pool and the legacy boost pool
/// proportionally to their available liquidity, with the lending pool guaranteed
/// to cover at least `min_lending_share` of `required_amount` whenever it has
/// enough capacity to do so. Neither pool is ever asked for more than its
/// available liquidity, and the two returned values always sum to
/// `required_amount`.
///
/// Returns `SplitRequiredAmountError` if the combined liquidity of both pools
/// is insufficient to cover `required_amount`.
fn try_split_required_amount(
	required_amount: AssetAmount,
	lending_available: AssetAmount,
	boost_available: AssetAmount,
	min_lending_share: Percent,
) -> Result<(AssetAmount, AssetAmount), SplitRequiredAmountError> {
	let total_available = lending_available.saturating_add(boost_available);

	ensure!(total_available >= required_amount, SplitRequiredAmountError);

	if total_available == 0 || required_amount == 0 {
		return Ok((0, 0));
	}

	// Proportional split (rounds down in favour of the boost pool).
	let proportional_lending =
		Permill::from_rational(lending_available, total_available) * required_amount;

	// Floor on the lending pool's share. `proportional_lending` is always
	// `<= lending_available` (since `required_amount <= lending + boost`), so the
	// final `.min(lending_available)` is what enforces the cap when the floor exceeds it.
	let min_lending = min_lending_share * required_amount;

	let lending_pool_principal = proportional_lending.max(min_lending).min(lending_available);
	let boost_pool_principal =
		required_amount.saturating_sub(lending_pool_principal).min(boost_available);
	// Handle any rounding shortfall (or lending slack freed up by the boost-pool clamp)
	// by pushing the remainder back to the lending pool. Guaranteed to be within
	// `lending_available` because total liquidity is at least `required_amount`.
	let lending_pool_principal = required_amount.saturating_sub(boost_pool_principal);

	Ok((lending_pool_principal, boost_pool_principal))
}

#[cfg(test)]
mod split_tests {
	use super::*;

	const MIN_LENDING_SHARE: Percent = Percent::from_percent(30);

	fn split(
		required: AssetAmount,
		lending: AssetAmount,
		boost: AssetAmount,
	) -> (AssetAmount, AssetAmount) {
		try_split_required_amount(required, lending, boost, MIN_LENDING_SHARE).unwrap()
	}

	#[test]
	fn proportional_split_when_both_pools_have_slack() {
		// 100 + 100 available, need 150 -> 75/75 split (50% each).
		assert_eq!(split(150, 100, 100), (75, 75));
	}

	#[test]
	fn min_lending_share_enforced_when_lending_is_small_relative_to_boost() {
		// Proportional would give lending only 200 (= 20%) but the 30% minimum kicks
		// in and lending covers 300 while boost covers the remaining 700.
		assert_eq!(split(1000, 500, 2000), (300, 700));
	}

	#[test]
	fn lending_dominates_but_boost_still_gets_its_proportional_share() {
		// Proportional: lending ~99%, boost ~1%.
		assert_eq!(split(100, 990, 10), (99, 1));
	}

	#[test]
	fn lending_falls_below_min_when_it_lacks_liquidity() {
		// Lending has only 50, which is below 30% of 1000 = 300. It still contributes
		// everything it has and the boost pool covers the remainder.
		assert_eq!(split(1000, 50, 10_000), (50, 950));
	}

	#[test]
	fn boost_pool_only_when_lending_is_empty() {
		assert_eq!(split(1000, 0, 1000), (0, 1000));
	}

	#[test]
	fn lending_pool_only_when_boost_is_empty() {
		assert_eq!(split(1000, 1000, 0), (1000, 0));
	}

	#[test]
	fn zero_required_amount() {
		assert_eq!(split(0, 500, 500), (0, 0));
	}

	#[test]
	fn rounding_shortfall_pushed_back_to_lending() {
		// `Permill::from_rational(1, 999_999)` rounds down to 1 ppm, so proportional
		// is 0. min_lending is 30% * 999_999 = 299_999 (capped at lending_available=1),
		// so lending contributes 1 and boost covers the remaining 999_998.
		assert_eq!(split(999_999, 1, 999_998), (1, 999_998));
	}

	#[test]
	fn principals_never_exceed_available_liquidity() {
		// Stress with exact-fit liquidity: total == required.
		assert_eq!(split(1_000, 300, 700), (300, 700));
		assert_eq!(split(1_000, 700, 300), (700, 300));
		assert_eq!(split(1_000, 1_000, 0), (1_000, 0));
		assert_eq!(split(1_000, 0, 1_000), (0, 1_000));
	}

	#[test]
	fn min_lending_share_is_configurable() {
		// With a 50% minimum, lending must cover 500 even though proportional is ~230.
		assert_eq!(
			try_split_required_amount(1000, 600, 2000, Percent::from_percent(50)).unwrap(),
			(500, 500)
		);
		// With a 0% minimum, only proportional applies.
		assert_eq!(
			try_split_required_amount(1000, 200, 2000, Percent::from_percent(0)).unwrap(),
			(91, 909)
		);
		// With a 100% minimum, lending covers everything when it has the liquidity.
		assert_eq!(
			try_split_required_amount(1000, 1200, 2000, Percent::from_percent(100)).unwrap(),
			(1000, 0)
		);
		assert_eq!(
			try_split_required_amount(1000, 999, 1000, Percent::from_percent(100)).unwrap(),
			(999, 1)
		);
	}

	#[test]
	fn errors_when_total_liquidity_is_insufficient() {
		assert!(try_split_required_amount(1000, 400, 500, MIN_LENDING_SHARE).is_err());
		assert!(try_split_required_amount(1, 0, 0, MIN_LENDING_SHARE).is_err());
	}
}

#[test]
fn test_fee_math() {
	assert_eq!(fee_from_boosted_amount(1_000_000, 10), 1_000);
}
