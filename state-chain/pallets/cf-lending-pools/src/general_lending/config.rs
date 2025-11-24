use super::*;

/// Interest "curve" is defined as two linear segments. One is in effect from 0% to
/// `junction_utilisation`, and the second is in effect from `junction_utilisation` to 100%.
#[derive(
	Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, Serialize, Deserialize,
)]
pub struct InterestRateConfiguration {
	pub interest_at_zero_utilisation: Permill,
	pub junction_utilisation: Permill,
	pub interest_at_junction_utilisation: Permill,
	pub interest_at_max_utilisation: Permill,
}

impl InterestRateConfiguration {
	pub fn validate(&self) -> DispatchResult {
		// Ensure that the interest rate increases with utilization, which
		// we rely on when interpolating the curve.
		ensure!(
			self.interest_at_zero_utilisation <= self.interest_at_junction_utilisation &&
				self.interest_at_junction_utilisation <= self.interest_at_max_utilisation,
			"Invalid curve"
		);

		Ok(())
	}
}

#[derive(
	Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, Serialize, Deserialize,
)]
pub struct LendingPoolConfiguration {
	pub origination_fee: Permill,
	/// Portion of the amount of principal asset obtained via liquidation that's
	/// paid as a fee (instead of reducing the loan's principal)
	pub liquidation_fee: Permill,
	/// Determines how interest rate is calculated based on the utilization rate.
	pub interest_rate_curve: InterestRateConfiguration,
}

#[derive(
	Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, Serialize, Deserialize,
)]
pub struct LtvThresholds {
	/// Borrowers aren't allowed to borrow more (or withdraw collateral) if their Loan-to-value
	/// ratio (principal/collateral) would exceed this threshold.
	pub target: Permill,
	/// Reaching this threshold will trigger a top-up of the collateral
	pub topup: Option<Permill>,
	/// Reaching this threshold will trigger soft liquidation account's loans
	pub soft_liquidation: Permill,
	/// If a loan that's being liquidated reaches this threshold, it will be considered
	/// "healthy" again and the liquidation will be aborted. This is meant to be slightly
	/// lower than the soft threshold to avoid frequent oscillations between liquidating and
	/// not liquidating.
	pub soft_liquidation_abort: Permill,
	/// Reaching this threshold will trigger hard liquidation of the loan
	pub hard_liquidation: Permill,
	/// Same as overcollateralisation_soft_liquidation_abort_threshold, but for
	/// transitioning from hard to soft liquidation
	pub hard_liquidation_abort: Permill,
	/// The max value for LTV that doesn't lead to borrowers paying extra interest to the network
	/// on their collateral
	pub low_ltv: Permill,
}

impl LtvThresholds {
	pub fn validate(&self) -> DispatchResult {
		ensure!(self.soft_liquidation <= self.hard_liquidation, "Invalid LTV thresholds");
		ensure!(
			self.topup
				.is_none_or(|topup| self.target <= topup && topup <= self.soft_liquidation),
			"Invalid LTV thresholds"
		);

		ensure!(
			self.hard_liquidation_abort < self.hard_liquidation &&
				self.soft_liquidation_abort < self.soft_liquidation,
			"Invalid LTV thresholds"
		);

		Ok(())
	}
}

#[derive(
	Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, Serialize, Deserialize,
)]
pub struct NetworkFeeContributions {
	/// A fixed % that's added to the base interest to get the total borrow rate (as a % on the
	/// principal paid every year)
	pub extra_interest: Permill,
	/// The % of the origination fee that should be taken as a network fee.
	pub from_origination_fee: Permill,
	/// The % of the liquidation fee that should be taken as a network fee.
	pub from_liquidation_fee: Permill,
	/// Max value of additional interest/pentalty (at LTV approaching 0%)
	pub low_ltv_penalty_max: Permill,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct LendingConfiguration {
	/// This configuration is used unless it is overridden in `pool_config_overrides`.
	pub default_pool_config: LendingPoolConfiguration,
	/// Determines when events like liquidation should be triggered based on the account's
	/// loan-to-value ratio.
	pub ltv_thresholds: LtvThresholds,
	/// Determines what portion of each fee type should be taken as a network fee.
	pub network_fee_contributions: NetworkFeeContributions,
	/// Determines how frequently (in blocks) we check if fees should be swapped into the
	/// pools asset
	pub fee_swap_interval_blocks: u32,
	/// Determines how frequently (in blocks) we calculate and record interest payments from loans.
	pub interest_payment_interval_blocks: u32,
	/// If loan account's owed interest reaches this threshold, it will be taken from the account's
	/// collateral
	pub interest_collection_threshold_usd: AssetAmount,
	/// Fees collected in some asset will be swapped into the pool's asset once their usd value
	/// reaches this threshold
	pub fee_swap_threshold_usd: AssetAmount,
	/// Soft liquidation will be executed with this oracle slippage limit
	pub soft_liquidation_max_oracle_slippage: BasisPoints,
	/// Hard liquidation will be executed with this oracle slippage limit
	pub hard_liquidation_max_oracle_slippage: BasisPoints,
	/// Soft liquidation swaps will use chunks that are equivalent to this amount of USD
	pub soft_liquidation_swap_chunk_size_usd: AssetAmount,
	/// Hard liquidation swaps will use chunks that are equivalent to this amount of USD
	pub hard_liquidation_swap_chunk_size_usd: AssetAmount,
	/// All fee swaps from lending will be executed with this oracle slippage limit
	pub fee_swap_max_oracle_slippage: BasisPoints,
	/// If set for a pool/asset, this configuration will be used instead of the default
	pub pool_config_overrides: BTreeMap<Asset, LendingPoolConfiguration>,
	/// Minimum amount of principal that a loan must have at all times.
	pub minimum_loan_amount_usd: AssetAmount,
	/// Minimum amount of that can be added to a lending pool. When removing funds, the user
	/// can't leave less than this amount in the pool (they should remove all funds instead).
	pub minimum_supply_amount_usd: AssetAmount,
	/// Minimum equivalent amount of principal that can be used to expand or repay an existing
	/// loan.
	pub minimum_update_loan_amount_usd: AssetAmount,
	/// Minimum equivalent amount of collateral that can be added or removed from a loan account.
	pub minimum_update_collateral_amount_usd: AssetAmount,
}

impl LendingConfiguration {
	pub fn get_config_for_asset(&self, asset: Asset) -> &LendingPoolConfiguration {
		self.pool_config_overrides.get(&asset).unwrap_or(&self.default_pool_config)
	}

	pub fn derive_interest_rate_per_year(&self, asset: Asset, utilisation: Permill) -> Permill {
		let InterestRateConfiguration {
			interest_at_zero_utilisation,
			junction_utilisation,
			interest_at_junction_utilisation,
			interest_at_max_utilisation,
		} = self.get_config_for_asset(asset).interest_rate_curve;

		if utilisation < junction_utilisation {
			interpolate_linear_segment(
				Permill::zero(),
				junction_utilisation,
				interest_at_zero_utilisation,
				interest_at_junction_utilisation,
				utilisation,
			)
		} else {
			interpolate_linear_segment(
				junction_utilisation,
				Permill::one(),
				interest_at_junction_utilisation,
				interest_at_max_utilisation,
				utilisation,
			)
		}
	}

	fn interest_per_year_to_per_payment_interval(
		&self,
		interest_per_year: Permill,
		interval_blocks: u32,
	) -> Perquintill {
		use cf_primitives::BLOCKS_IN_YEAR;

		Perquintill::from_parts(
			(interest_per_year.deconstruct() as u64 *
				(Perquintill::ACCURACY / Permill::ACCURACY as u64)) /
				(BLOCKS_IN_YEAR / interval_blocks) as u64,
		)
	}

	/// Computes the interest rate to be paid each payment interval. Uses Perquintill for better
	/// precision as the value is likely to be a very small fraction due to the interval being
	/// short.
	pub fn derive_base_interest_rate_per_payment_interval(
		&self,
		asset: Asset,
		utilisation: Permill,
		interval_blocks: u32,
	) -> Perquintill {
		let interest_rate = self.derive_interest_rate_per_year(asset, utilisation);

		self.interest_per_year_to_per_payment_interval(interest_rate, interval_blocks)
	}

	pub fn derive_network_interest_rate_per_payment_interval(
		&self,
		interval_blocks: u32,
	) -> Perquintill {
		self.interest_per_year_to_per_payment_interval(
			self.network_fee_contributions.extra_interest,
			interval_blocks,
		)
	}

	/// Computes an additional annual interest/penalty for loan accounts with LTV below
	/// `low_ltv` to incentivise capital efficiency. The penalty decreases linearly
	/// from `low_ltv_penalty_max` at 0% LTV to zero at `low_ltv` threshold.
	fn derive_low_ltv_interest_rate_per_year(&self, ltv: FixedU64) -> Permill {
		let ltv: Permill = ltv.into_clamped_perthing();

		if ltv >= self.ltv_thresholds.low_ltv {
			return Permill::zero();
		}

		interpolate_linear_segment(
			Permill::zero(),
			self.ltv_thresholds.low_ltv,
			self.network_fee_contributions.low_ltv_penalty_max,
			Permill::zero(),
			ltv,
		)
	}

	pub fn derive_low_ltv_penalty_rate_per_payment_interval(
		&self,
		ltv: FixedU64,
		interval_blocks: u32,
	) -> Perquintill {
		let interest_rate = self.derive_low_ltv_interest_rate_per_year(ltv);
		self.interest_per_year_to_per_payment_interval(interest_rate, interval_blocks)
	}

	pub fn origination_fee(&self, asset: Asset) -> Permill {
		self.get_config_for_asset(asset).origination_fee
	}

	pub fn liquidation_fee(&self, asset: Asset) -> Permill {
		self.get_config_for_asset(asset).liquidation_fee
	}
}

/// Computes the value of `f(x)` where f is a linear function defined by two points:
/// (x0, y0) and (x1, y1). The code assumes x0 <= x <= x1 and x0 != x1.
fn interpolate_linear_segment(
	x0: Permill,
	x1: Permill,
	y0: Permill,
	y1: Permill,
	x: Permill,
) -> Permill {
	if x0 > x || x > x1 || x0 == x1 {
		log_or_panic!("Invalid parameters");
		return Permill::zero();
	}

	let y0 = FixedI64::from(y0);
	let y1 = FixedI64::from(y1);
	let x0 = FixedI64::from(x0);
	let x1 = FixedI64::from(x1);
	let x = FixedI64::from(x);

	let slope = (y1 - y0) / (x1 - x0);

	let delta = slope * (x - x0);

	y0.saturating_add(delta).into_clamped_perthing()
}

#[cfg(test)]
mod tests {

	use super::*;
	use crate::LENDING_DEFAULT_CONFIG as CONFIG;

	#[test]
	fn linear_segment_interpolation() {
		// Linear segment starts at 0% and ends at 90%
		assert_eq!(
			interpolate_linear_segment(
				Permill::from_percent(0),
				Permill::from_percent(90),
				Permill::from_percent(2),
				Permill::from_percent(8),
				Permill::from_percent(45),
			),
			Permill::from_parts(49_999) // ~5%
		);

		// Linear segment starts at 90% and ends at 100%
		assert_eq!(
			interpolate_linear_segment(
				Permill::from_percent(90),
				Permill::from_percent(100),
				Permill::from_percent(8),
				Permill::from_percent(50),
				Permill::from_percent(95),
			),
			Permill::from_percent(29)
		);

		// Linear segment from 0% to 100% and zero slope
		assert_eq!(
			interpolate_linear_segment(
				Permill::from_percent(0),
				Permill::from_percent(100),
				Permill::from_percent(5),
				Permill::from_percent(5),
				Permill::from_percent(75),
			),
			Permill::from_percent(5)
		);

		// === Some linear segments with a negative slope ===
		assert_eq!(
			interpolate_linear_segment(
				Permill::from_percent(0),
				Permill::from_percent(50),
				Permill::from_percent(50),
				Permill::from_percent(10),
				Permill::from_percent(25),
			),
			Permill::from_percent(30)
		);

		assert_eq!(
			interpolate_linear_segment(
				Permill::from_percent(0),
				Permill::from_percent(50),
				Permill::from_percent(50),
				Permill::from_percent(10),
				Permill::from_percent(0),
			),
			Permill::from_percent(50)
		);

		assert_eq!(
			interpolate_linear_segment(
				Permill::from_percent(0),
				Permill::from_percent(50),
				Permill::from_percent(50),
				Permill::from_percent(10),
				Permill::from_percent(50),
			),
			Permill::from_percent(10)
		);
	}

	#[test]
	fn interest_rate_curve() {
		// The exact asset is not important for this test
		let asset = Asset::Btc;

		assert_eq!(
			CONFIG.derive_interest_rate_per_year(asset, Permill::from_percent(0)),
			Permill::from_percent(2)
		);

		assert_eq!(
			CONFIG.derive_interest_rate_per_year(asset, Permill::from_percent(45)),
			Permill::from_parts(49_999) // (2% + 8%) / 2 = 5%
		);

		assert_eq!(
			CONFIG.derive_interest_rate_per_year(asset, Permill::from_percent(90)),
			Permill::from_percent(8)
		);

		assert_eq!(
			CONFIG.derive_interest_rate_per_year(asset, Permill::from_percent(95)),
			Permill::from_percent(29) // (8% + 50%) / 2 = 29%
		);

		assert_eq!(
			CONFIG.derive_interest_rate_per_year(asset, Permill::from_percent(100)),
			Permill::from_percent(50)
		);
	}

	#[test]
	fn derive_extra_interest_from_low_ltv() {
		assert_eq!(
			CONFIG.derive_low_ltv_interest_rate_per_year(FixedU64::zero()),
			Permill::from_percent(1)
		);

		assert_eq!(
			CONFIG.derive_low_ltv_interest_rate_per_year(FixedU64::from_rational(10, 100)),
			Permill::from_parts(8_000) // 0.8%
		);

		assert_eq!(
			CONFIG.derive_low_ltv_interest_rate_per_year(FixedU64::from_rational(25, 100)),
			Permill::from_parts(5_000) // 0.5%
		);

		// Any value above 50% LTV should result in 0% additional interest:
		assert_eq!(
			CONFIG.derive_low_ltv_interest_rate_per_year(FixedU64::from_rational(50, 100)),
			Permill::from_percent(0)
		);

		assert_eq!(
			CONFIG.derive_low_ltv_interest_rate_per_year(FixedU64::from_rational(80, 100)),
			Permill::from_percent(0)
		);

		assert_eq!(
			CONFIG.derive_low_ltv_interest_rate_per_year(FixedU64::from_rational(120, 100)),
			Permill::from_percent(0)
		);
	}
}
