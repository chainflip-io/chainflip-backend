macro_rules! derive_scalar_impls {
	($scalar: path) => {
		impl Default for $scalar {
			fn default() -> Self {
				Self::zero()
			}
		}

		impl Drop for $scalar {
			fn drop(&mut self) {
				use zeroize::Zeroize;
				self.zeroize();
			}
		}

		impl zeroize::ZeroizeOnDrop for $scalar {}

		impl std::ops::Add for $scalar {
			type Output = $scalar;

			fn add(self, rhs: Self) -> Self::Output {
				&self + &rhs
			}
		}

		impl std::ops::Add<&$scalar> for $scalar {
			type Output = $scalar;

			fn add(self, rhs: &$scalar) -> Self::Output {
				&self + rhs
			}
		}

		impl std::ops::Sub for $scalar {
			type Output = $scalar;

			fn sub(self, rhs: Self) -> Self::Output {
				&self - &rhs
			}
		}

		impl std::iter::Sum for $scalar {
			fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
				iter.fold(<$scalar>::zero(), |a, b| a + b)
			}
		}

		impl std::ops::Mul for Scalar {
			type Output = Scalar;

			fn mul(self, rhs: Self) -> Self::Output {
				&self * &rhs
			}
		}

		impl std::ops::Mul<&Scalar> for Scalar {
			type Output = Scalar;

			fn mul(self, rhs: &Scalar) -> Self::Output {
				&self * rhs
			}
		}
	};
}

macro_rules! derive_point_impls {
	($point: path, $scalar: path) => {
		impl Default for $point {
			fn default() -> Self {
				Self::point_at_infinity()
			}
		}

		impl zeroize::DefaultIsZeroes for $point {}

		impl std::iter::Sum for $point {
			fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
				// "Point at infinity" corresponds to "zero" on
				// an elliptic curve
				iter.fold(Self::point_at_infinity(), |a, b| a + b)
			}
		}
	};
}
