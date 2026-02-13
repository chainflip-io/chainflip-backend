use frame_support::sp_runtime::{helpers_128bit::multiply_by_rational_with_rounding, Rounding};

use super::*;

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
					let Some(contribution) = BoostedDeposits::<T>::get(asset, deposit_id)
						.and_then(|pools| pools.get(&tier).cloned())
					else {
						return (deposit_id, BTreeMap::default());
					};

					let BoostPoolContribution { boosted_amount, network_fee, .. } = contribution;

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
		let mut remaining_amount = deposit_amount;
		let mut total_fee_amount: AssetAmount = 0;

		let mut used_pools = BTreeMap::new();

		let network_fee_portion = BoostConfig::<T>::get().network_fee_deduction_from_boost_percent;

		let sorted_boost_pools = BoostPools::<T>::iter_prefix(asset)
			.map(|(tier, pool)| (tier, pool.core_pool_id))
			.collect::<BTreeMap<_, _>>();

		for (boost_tier, core_pool_id) in sorted_boost_pools {
			if boost_tier > max_boost_fee_bps {
				break
			}

			let Some((loan_id, boosted_amount, fee)) =
				CorePools::<T>::mutate(asset, core_pool_id, |pool| {
					let core_pool: &mut CoreLendingPool<_> = match pool {
						Some(pool) if pool.get_available_amount() == Zero::zero() => {
							return Ok::<_, DispatchError>(None);
						},
						None => {
							// Pool not existing for some reason is equivalent to not having funds:
							return Ok::<_, DispatchError>(None);
						},
						Some(pool) => pool,
					};

					// 1. Derive the amount that needs to be borrowed:
					let full_amount_fee = fee_from_boosted_amount(remaining_amount, boost_tier);
					let required_amount = remaining_amount.saturating_sub(full_amount_fee);

					let available_amount = core_pool.get_available_amount();

					let (amount_to_provide, fee_amount) = if available_amount >= required_amount {
						// Will borrow full required amount
						(required_amount, full_amount_fee)
					} else {
						// Will only borrow what is available
						let amount_to_provide = available_amount;
						let fee = fee_from_provided_amount(amount_to_provide, boost_tier)?;

						(amount_to_provide, fee)
					};

					let loan_id =
						core_pool.new_loan(amount_to_provide, LoanUsage::Boost(deposit_id))?;

					Ok(Some((loan_id, amount_to_provide.saturating_add(fee_amount), fee_amount)))
				})?
			else {
				// Can't use the current pool, moving on to the next
				continue;
			};

			// NOTE: A portion of the boost pool fees will be charged as network fee:
			let network_fee = network_fee_portion * fee;
			used_pools.insert(
				boost_tier,
				BoostPoolContribution { core_pool_id, loan_id, boosted_amount, network_fee },
			);

			remaining_amount.saturating_reduce(boosted_amount);
			total_fee_amount.saturating_accrue(fee);

			if remaining_amount == 0u32.into() {
				let boost_output = BoostOutcome {
					used_pools: used_pools
						.iter()
						.map(|(tier, pool)| (*tier, pool.boosted_amount))
						.collect(),
					total_fee: total_fee_amount,
				};

				BoostedDeposits::<T>::insert(asset, deposit_id, used_pools);
				return Ok(boost_output);
			}
		}

		Err(Error::<T>::InsufficientBoostLiquidity.into())
	}

	fn finalise_boost(deposit_id: PrewitnessedDepositId, asset: Asset) -> BoostFinalisationOutcome {
		let Some(pool_contributions) = BoostedDeposits::<T>::take(asset, deposit_id) else {
			return Default::default();
		};

		let mut total_network_fee = 0;

		for BoostPoolContribution { core_pool_id, loan_id, boosted_amount, network_fee } in
			pool_contributions.values()
		{
			total_network_fee += network_fee;

			CorePools::<T>::mutate(asset, core_pool_id, |pool| {
				if let Some(pool) = pool {
					for (booster_id, unlocked_amount) in
						pool.make_repayment(*loan_id, boosted_amount.saturating_sub(*network_fee))
					{
						T::Balance::credit_account(&booster_id, asset, unlocked_amount);
					}
					pool.finalise_loan(*loan_id);
				}
			});
		}

		BoostFinalisationOutcome { network_fee: total_network_fee }
	}

	fn process_deposit_as_lost(deposit_id: PrewitnessedDepositId, asset: Asset) {
		let Some(pool_contributions) = BoostedDeposits::<T>::take(asset, deposit_id) else {
			log_or_panic!("Boost record for a lost deposit not found: {}", deposit_id);
			return;
		};

		for BoostPoolContribution { core_pool_id, loan_id, .. } in pool_contributions.values() {
			CorePools::<T>::mutate(asset, core_pool_id, |pool| {
				if let Some(pool) = pool {
					pool.finalise_loan(*loan_id);
				}
			});
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
			BoostedDeposits::<T>::iter_prefix(asset).fold(0, |acc, (_, pool_contributions)| {
				let in_boosted_deposit = pool_contributions.iter().fold(
					0,
					|acc,
					 (
						_,
						BoostPoolContribution {
							core_pool_id,
							loan_id,
							boosted_amount,
							network_fee,
						},
					)| {
						let Some(core_pool) = CorePools::<T>::get(asset, core_pool_id) else {
							return 0;
						};

						let Some(loan) = core_pool.pending_loans.get(loan_id) else { return 0 };

						let Some(share) = loan.shares.get(who) else { return 0 };

						acc + *share * boosted_amount.saturating_sub(*network_fee)
					},
				);

				acc + in_boosted_deposit
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

/// Unlike `fee_from_boosted_amount`, the boosted amount is not known here
/// so we have to calculate it first from the provided amount in order to
/// calculate the boost fee amount.
fn fee_from_provided_amount(
	provided_amount: AssetAmount,
	fee_bps: u16,
) -> Result<AssetAmount, &'static str> {
	// Compute `boosted = provided / (1 - fee)`
	let boosted_amount = {
		const BASIS_POINTS_MAX: u16 = 10_000;

		let inverse_fee = BASIS_POINTS_MAX.saturating_sub(fee_bps);

		multiply_by_rational_with_rounding(
			provided_amount,
			BASIS_POINTS_MAX as u128,
			inverse_fee as u128,
			Rounding::Down,
		)
		.ok_or("invalid fee")?
	};

	let fee_amount = boosted_amount.checked_sub(provided_amount).ok_or("invalid fee")?;

	Ok(fee_amount)
}

#[test]
fn test_fee_math() {
	assert_eq!(fee_from_boosted_amount(1_000_000, 10), 1_000);

	assert_eq!(fee_from_provided_amount(1_000_000, 10), Ok(1_001));
}
