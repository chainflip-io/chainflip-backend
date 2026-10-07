use super::*;

define_wrapper_type!(SignedBasisPoints, i32, extra_derives: Serialize, Deserialize, PartialOrd, Ord);
define_wrapper_type!(SignedHundredthBasisPoints, i32, extra_derives: Serialize, Deserialize, PartialOrd, Ord);

impl SignedBasisPoints {
	pub const MAX: Self = SignedBasisPoints(u16::MAX as i32);
	pub const MIN: Self = SignedBasisPoints(-(u16::MAX as i32));

	pub fn positive_slippage(bps: BasisPoints) -> Self {
		SignedBasisPoints(bps as i32)
	}
	pub fn negative_slippage(bps: BasisPoints) -> Self {
		SignedBasisPoints(-(bps as i32))
	}
}

impl SignedHundredthBasisPoints {
	pub const MAX: Self = SignedHundredthBasisPoints(u16::MAX as i32 * 100);
	pub const MIN: Self = SignedHundredthBasisPoints(-(u16::MAX as i32 * 100));

	/// Rounds towards the worst case (i.e. away from zero) and converts into
	/// [SignedBasisPoints], clamping to the valid range if necessary.
	pub fn pessimistic_rounded_into(&self) -> SignedBasisPoints {
		let rounded = if self.is_negative() { self.0.div_floor(100) } else { self.0.div_ceil(100) };
		SignedBasisPoints(rounded.clamp(SignedBasisPoints::MIN.0, SignedBasisPoints::MAX.0))
	}

	/// Combines the deltas of two consecutive swap legs into the delta of the whole route:
	/// `(1 + a)(1 + b) - 1 = a + b + a·b`. The cross term is rounded down (towards the worst
	/// case) and the result saturates to the `i32` range.
	pub fn compound(&self, other: &SignedHundredthBasisPoints) -> SignedHundredthBasisPoints {
		const ONE: i64 = 100 * ONE_AS_BASIS_POINTS as i64;
		let (a, b) = (self.0 as i64, other.0 as i64);
		SignedHundredthBasisPoints(
			a.saturating_add(b)
				.saturating_add(a.saturating_mul(b).div_floor(ONE))
				.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
		)
	}
}
impl From<SignedBasisPoints> for SignedHundredthBasisPoints {
	fn from(bps: SignedBasisPoints) -> Self {
		SignedHundredthBasisPoints((bps.0) * 100)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn signed_bps_to_hundredth_bps_conversion() {
		assert_eq!(SignedHundredthBasisPoints::from(SignedBasisPoints(0)).0, 0);
		assert_eq!(SignedHundredthBasisPoints::from(SignedBasisPoints(1)).0, 100);
		assert_eq!(SignedHundredthBasisPoints::from(SignedBasisPoints(-1)).0, -100);
		assert_eq!(SignedHundredthBasisPoints::from(SignedBasisPoints(123)).0, 12_300);
	}

	#[test]
	fn pessimistic_rounding_away_from_zero() {
		assert_eq!(SignedHundredthBasisPoints(0).pessimistic_rounded_into().0, 0);
		assert_eq!(SignedHundredthBasisPoints(1).pessimistic_rounded_into().0, 1);
		assert_eq!(SignedHundredthBasisPoints(100).pessimistic_rounded_into().0, 1);
		assert_eq!(SignedHundredthBasisPoints(101).pessimistic_rounded_into().0, 2);
		assert_eq!(SignedHundredthBasisPoints(-1).pessimistic_rounded_into().0, -1);
		assert_eq!(SignedHundredthBasisPoints(-100).pessimistic_rounded_into().0, -1);
		assert_eq!(SignedHundredthBasisPoints(-101).pessimistic_rounded_into().0, -2);
	}

	#[test]
	fn compound_same_sign_legs() {
		// -1% then -1% => 0.99 * 0.99 = 0.9801 => -1.99%
		assert_eq!(
			SignedHundredthBasisPoints(-10_000)
				.compound(&SignedHundredthBasisPoints(-10_000))
				.0,
			-19_900
		);
		// +1% then +1% => 1.01 * 1.01 = 1.0201 => +2.01%
		assert_eq!(
			SignedHundredthBasisPoints(10_000)
				.compound(&SignedHundredthBasisPoints(10_000))
				.0,
			20_100
		);
		assert_eq!(
			SignedHundredthBasisPoints(-1_234).compound(&SignedHundredthBasisPoints(0)).0,
			-1_234
		);
	}

	#[test]
	fn compound_opposite_sign_legs() {
		// -50% then +50% => 0.5 * 1.5 = 0.75 => -25%
		assert_eq!(
			SignedHundredthBasisPoints(-500_000)
				.compound(&SignedHundredthBasisPoints(500_000))
				.0,
			-250_000
		);
		// -98% then +97% sums to -1%, but 0.02 * 1.97 = 0.0394 => -96.06%
		assert_eq!(
			SignedHundredthBasisPoints(-980_000)
				.compound(&SignedHundredthBasisPoints(970_000))
				.0,
			-960_600
		);
	}

	#[test]
	fn compound_rounds_cross_term_down() {
		// The cross term is -1/1_000_000 hundredth bps, which rounds down to -1: -1 + 1 - 1.
		assert_eq!(SignedHundredthBasisPoints(-1).compound(&SignedHundredthBasisPoints(1)).0, -1);
		// The cross term is +1/1_000_000 hundredth bps, which rounds down to 0: 1 + 1 + 0.
		assert_eq!(SignedHundredthBasisPoints(1).compound(&SignedHundredthBasisPoints(1)).0, 2);
	}

	#[test]
	fn compound_saturates() {
		assert_eq!(
			SignedHundredthBasisPoints(i32::MAX)
				.compound(&SignedHundredthBasisPoints(i32::MAX))
				.0,
			i32::MAX
		);
		// A total loss on one leg is a total loss on the route, however good the other leg is.
		assert_eq!(
			SignedHundredthBasisPoints(-1_000_000)
				.compound(&SignedHundredthBasisPoints(i32::MAX))
				.0,
			-1_000_000
		);
	}

	#[test]
	fn pessimistic_rounding_clamps_to_signed_basis_points_range() {
		assert_eq!(
			SignedHundredthBasisPoints(SignedBasisPoints::MAX.0 * 100 + 50)
				.pessimistic_rounded_into()
				.0,
			SignedBasisPoints::MAX.0
		);
		assert_eq!(
			SignedHundredthBasisPoints(SignedBasisPoints::MIN.0 * 100 - 50)
				.pessimistic_rounded_into()
				.0,
			SignedBasisPoints::MIN.0
		);
	}
}
