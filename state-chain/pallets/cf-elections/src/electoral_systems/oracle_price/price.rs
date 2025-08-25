// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
use core::ops::Div;
use core::ops::{Add, Mul, RangeInclusive, Sub};

use crate::{
	electoral_systems::oracle_price::{primitives::BasisPoints, state_machine::PriceTrait},
	generic_tools::*,
};
use cf_amm_math::{mul_div_floor_checked, PRICE_FRACTIONAL_BITS};
use cf_primitives::Price;
#[cfg(test)]
use proptest::prelude::Strategy;
use sp_core::U256;

#[derive(Clone)]
pub struct PriceUnit {
	pub base_asset: PriceAsset,
	pub quote_asset: PriceAsset,
}

impl PriceUnit {
	pub fn get_base10_exponent(&self) -> i16 {
		self.base_asset.decimals() as i16 - self.quote_asset.decimals() as i16
	}
}

derive_common_traits! {
	#[derive(Copy, PartialOrd, Ord, TypeInfo)]
	pub enum PriceAsset {
		Btc,
		Eth,
		Sol,
		Usdc,
		Usdt,
		Usd,
		Fine,
	}
}

impl PriceAsset {
	pub fn decimals(&self) -> u8 {
		use PriceAsset::*;
		match self {
			Btc => 8,
			Eth => 18,
			Sol => 9,
			Usdc => 6,
			Usdt => 6,
			Usd => 6,
			Fine => 0,
		}
	}
}

pub type U256Base = [u64; 4];
type Denom = u128;

pub fn denom(d: Denom) -> U256 {
	U256::from(d) + 1
}

derive_common_traits! {
	/// Fixed-point fraction type with strongly typed denominator. The internal representation
	/// of the value is a U256, the const parameter `U: u128` defines the how the integer should
	/// be interpreted: as the numerator of a fraction with a denominator of `U + 1`.
	///
	/// This means:
	///  - `Fraction(1) : Fraction<3>` represents the fraction 1/4
	///  - `Fraction(1) : Fraction<99>` represents the fraction 1/100
	///  - `Fraction(15) : Fraction<99>` represents the fraction 15/100
	///  - `Fraction(3500) : Fraction<u128::MAX>` represents the fraction 3500/2^128
	///
	/// Note, `Fraction<U>` represents a denominator of `U + 1`, this has two advantages:
	///  - a denominator of `0` is automatically impossible
	///  - we can use `u128` as type of `U` and still represent a denominator of 2^128 = u128::MAX + 1,
	///    necessary to mimic the representation of the cf_amm_math::Price type.
	///
	#[derive(Default, PartialOrd, Ord, TypeInfo)]
	pub struct Fraction<const U: Denom>(pub U256);
}

impl<const U: Denom> Fraction<U> {
	pub fn from_raw(raw: U256) -> Self {
		Fraction(raw)
	}

	pub fn integer<Int: Into<U256>>(int: Int) -> Self {
		Fraction(int.into().saturating_mul(denom(U)))
	}

	pub fn one() -> Self {
		Fraction(denom(U))
	}

	pub fn denominator() -> U256 {
		denom(U)
	}

	pub fn convert<const V: Denom>(self) -> Option<Fraction<V>> {
		Some(Fraction(mul_div_floor_checked(self.0, denom(V), denom(U))?))
	}
}

impl<const U: Denom> Add<Fraction<U>> for Fraction<U> {
	type Output = Fraction<U>;

	fn add(self, rhs: Fraction<U>) -> Self::Output {
		Fraction(self.0.saturating_add(rhs.0))
	}
}

impl<const U: Denom> Add<Fraction<U>> for &Fraction<U> {
	type Output = Fraction<U>;

	fn add(self, rhs: Fraction<U>) -> Self::Output {
		Fraction(self.0.saturating_add(rhs.0))
	}
}

impl<const U: Denom> Sub<Fraction<U>> for Fraction<U> {
	type Output = Fraction<U>;

	fn sub(self, rhs: Fraction<U>) -> Self::Output {
		Fraction(self.0.saturating_sub(rhs.0))
	}
}

impl<const U: Denom> Sub<Fraction<U>> for &Fraction<U> {
	type Output = Fraction<U>;

	fn sub(self, rhs: Fraction<U>) -> Self::Output {
		Fraction(self.0.saturating_sub(rhs.0))
	}
}

impl<const U: Denom, const V: Denom> Mul<Fraction<V>> for Fraction<U> {
	type Output = Option<Fraction<U>>;

	fn mul(self, rhs: Fraction<V>) -> Self::Output {
		Some(Fraction(mul_div_floor_checked(self.0, rhs.0, denom(V))?))
	}
}

impl<const U: Denom, const V: Denom> Mul<Fraction<V>> for &Fraction<U> {
	type Output = Option<Fraction<U>>;

	fn mul(self, rhs: Fraction<V>) -> Self::Output {
		Some(Fraction(mul_div_floor_checked(self.0, rhs.0, denom(V))?))
	}
}

#[cfg(test)]
impl<const N: u128> Arbitrary for Fraction<N> {
	type Parameters = ();

	fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
		use proptest::prelude::*;
		any::<[u64; 4]>().prop_map(U256).prop_map(Fraction)
	}

	type Strategy = impl Strategy<Value = Fraction<N>> + Clone + Send;
}

#[cfg(test)]
impl<const U: Denom> Div<u32> for Fraction<U> {
	type Output = Fraction<U>;

	fn div(self, rhs: u32) -> Self::Output {
		// WARNING: we have unwrap here only because this code is compiled only for test
		Fraction(mul_div_floor_checked(self.0, 1u128.into(), rhs).unwrap())
	}
}

/// This price type has the same encoding as the `cf_amm_math::Price` type we use internally.
pub type StatechainPrice = Fraction<{ u128::MAX }>;

/// WARNING, this assumes PRICE_FRACTIONAL_BITS = 128!
impl From<Fraction<{ u128::MAX }>> for Price {
	fn from(fraction: Fraction<{ u128::MAX }>) -> Price {
		debug_assert_eq!(PRICE_FRACTIONAL_BITS, 128);
		fraction.0
	}
}

pub fn convert_unit<const U: Denom>(
	price: Fraction<U>,
	from: PriceUnit,
	to: PriceUnit,
) -> Option<Fraction<U>> {
	let exponent_delta = to.get_base10_exponent() - from.get_base10_exponent();

	if exponent_delta < 0 {
		Some(Fraction(mul_div_floor_checked(
			price.0,
			1.into(),
			10u128.pow((-exponent_delta) as u32),
		)?))
	} else {
		Some(Fraction(mul_div_floor_checked(price.0, 10u128.pow(exponent_delta as u32).into(), 1)?))
	}
}

impl<const U: Denom> PriceTrait for Fraction<U> {
	fn to_price_range(&self, range: BasisPoints) -> Option<RangeInclusive<Self>> {
		let delta = (self * range.to_fraction())?;
		Some(self - delta.clone()..=self + delta)
	}
}

#[cfg(test)]
mod tests {
	use crate::electoral_systems::oracle_price::{price::Fraction, primitives::BasisPoints};
	use sp_core::U256;

	#[test]
	fn test_fraction_examples() {
		{
			fn check(input: &Fraction<99>, expected: u128) {
				assert_eq!(input.0, U256::from(expected))
			}

			// testing fractions with fractional part 100:
			let a: Fraction<99> = Fraction::integer(5);
			check(&a, 500);
			check(&(&a * a.clone()).unwrap(), 2500);

			let b: Fraction<99> = (&a * BasisPoints(1000).to_fraction()).unwrap();
			check(&b, 50);
			check(&(&b * b.clone()).unwrap(), 25);
			check(&(&b + b.clone()), 100);
			check(&(&b * Fraction::<99>::integer(4)).unwrap(), 200);
			check(&(&b * Fraction::<12345>::integer(4)).unwrap(), 200);
			check(&(&b - BasisPoints(1000).to_fraction().convert().unwrap()), 40);
		}

		{
			fn check(input: &Fraction<255>, expected: u128) {
				assert_eq!(input.0, U256::from(expected))
			}

			// testing fractions with fractional part 256 = 2^8:
			let a: Fraction<255> = Fraction::integer(5);
			check(&a, 5 << 8);
			check(&(&a * a.clone()).unwrap(), 25 << 8);

			// b = a / 4
			let b: Fraction<255> = (&a * Fraction::<255>((1u128 << 6).into())).unwrap();
			check(&b, 5 << 6);
			check(&(&b * b.clone()).unwrap(), 25 << 4);
			check(&(&b + b.clone()), 10 << 6);
			check(&(&b * Fraction::<255>::integer(4)).unwrap(), 20 << 6);
			check(&(&b * Fraction::<12345>::integer(4)).unwrap(), 20 << 6);
			check(&(&b - Fraction::<255>(1u128.into())), (5 << 6) - 1);
		}
	}
}
