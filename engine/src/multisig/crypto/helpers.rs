macro_rules! derive_scalar_impls {
    ($scalar: path) => {
        impl Default for $scalar {
            fn default() -> Self {
                <$scalar>::zero()
            }
        }

        impl Drop for $scalar {
            fn drop(&mut self) {
                self.zeroize();
            }
        }

        impl ZeroizeOnDrop for $scalar {}

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
