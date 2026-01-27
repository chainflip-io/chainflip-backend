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

	/// Checks whether the value of `self` breaches the given limit in
	/// [SignedBasisPoints]. If the limit is negative, checks whether `self` is less than the limit.
	/// If the limit is positive, checks whether `self` is greater than the limit.
	pub fn breaches_limit(&self, limit: SignedBasisPoints) -> bool {
		if limit.0 == 0 {
			return self.0 != 0;
		}

		let limit_in_hundredths = Self::from(limit);
		if limit_in_hundredths.is_negative() {
			*self < limit_in_hundredths
		} else {
			*self > limit_in_hundredths
		}
	}

	/// Rounds towards the worst case (i.e. away from zero) and converts into
	/// [SignedBasisPoints], clamping to the valid range if necessary.
	pub fn pessimistic_rounded_into(&self) -> SignedBasisPoints {
		let rounded = if self.is_negative() { self.0.div_floor(100) } else { self.0.div_ceil(100) };
		SignedBasisPoints(rounded.clamp(SignedBasisPoints::MIN.0, SignedBasisPoints::MAX.0))
	}

	pub fn saturating_add(&self, other: &SignedHundredthBasisPoints) -> SignedHundredthBasisPoints {
		SignedHundredthBasisPoints(self.0.saturating_add(other.0))
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

	#[test]
	fn breaches_limit_checks_sign_and_magnitude() {
		assert!(SignedHundredthBasisPoints(101).breaches_limit(SignedBasisPoints(1)));
		assert!(!SignedHundredthBasisPoints(100).breaches_limit(SignedBasisPoints(1)));
		assert!(!SignedHundredthBasisPoints(-100).breaches_limit(SignedBasisPoints(1)));
		assert!(!SignedHundredthBasisPoints(-101).breaches_limit(SignedBasisPoints(1)));
		assert!(!SignedHundredthBasisPoints(-1).breaches_limit(SignedBasisPoints(1)));
		assert!(!SignedHundredthBasisPoints(0).breaches_limit(SignedBasisPoints(1)));
		assert!(!SignedHundredthBasisPoints(0).breaches_limit(SignedBasisPoints(-1)));
		assert!(!SignedHundredthBasisPoints(-100).breaches_limit(SignedBasisPoints(-1)));
		assert!(SignedHundredthBasisPoints(-101).breaches_limit(SignedBasisPoints(-1)));
		assert!(SignedHundredthBasisPoints(1).breaches_limit(SignedBasisPoints(0)));
		assert!(SignedHundredthBasisPoints(-1).breaches_limit(SignedBasisPoints(0)));
		assert!(!SignedHundredthBasisPoints(0).breaches_limit(SignedBasisPoints(0)));
	}
}
