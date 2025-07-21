#[cfg(test)]
use core::ops::Div;
use core::ops::{Add, Mul, RangeInclusive, Sub};

use crate::{
	electoral_systems::oracle_price::{primitives::BasisPoints, state_machine::PriceTrait},
	generic_tools::*,
};
use cf_amm_math::{mul_div_floor, PRICE_FRACTIONAL_BITS};
use cf_primitives::{Asset, Price};
#[cfg(test)]
use proptest::prelude::Strategy;
use scale_info::prelude::marker::ConstParamTy;
use sp_core::U256;

use crate::electoral_systems::state_machine::common_imports::*;

#[cfg(test)]
use proptest_derive::Arbitrary;

pub struct PriceUnit {
	pub base_asset: PriceAsset,
	pub quote_asset: PriceAsset,
}

pub struct Base10Exponent(i16);

impl Base10Exponent {
	pub fn inv(&self) -> Self {
		Base10Exponent(-self.0)
	}
}

impl PriceUnit {
	pub fn get_exponent(&self) -> Base10Exponent {
		Base10Exponent(self.base_asset.decimals() as i16 - self.quote_asset.decimals() as i16)
	}
}

#[derive(ConstParamTy, PartialEq, Eq, Clone, Copy, PartialOrd, Ord)]
pub enum PriceAsset {
	Btc,
	Eth,
	Sol,
	Usdc,
	Usdt,
	Usd,
}

impl PriceAsset {
	pub fn decimals(&self) -> u8 {
		use PriceAsset::*;
		match self {
			Btc => 8,
			Eth => 12,
			Sol => 9,
			Usdc => 6,
			Usdt => 6,
			Usd => 6,
		}
	}
}

pub type U256Base = [u64; 4];
type Denom = u128;

pub fn denom(d: Denom) -> U256 {
	U256::from(d) + 1
}

def_derive! {
	/// Note that this can handle denominators of up to u128::MAX + 1
	#[derive(Default, PartialOrd, Ord, TypeInfo)]
	pub struct FractionImpl<const U: Denom>(pub U256);
}

// pub type Fraction<const U: Denom> = FractionImpl<{U - 1}>;

impl<const U: Denom> FractionImpl<U> {
	pub fn from_raw(raw: U256) -> Self {
		FractionImpl(raw)
	}

	pub fn integer<Int: Into<U256>>(int: Int) -> Self {
		FractionImpl(int.into() * denom(U))
	}

	pub fn one() -> Self {
		FractionImpl(denom(U))
	}

	pub fn denominator() -> U256 {
		denom(U)
	}

	/// This fails if the denominator isn't a multiple of `denom(U)`
	pub fn try_from_denominator_exact(numerator: u128, denominator: u128) -> Option<Self> {
		if denom(U) % denominator == 0.into() {
			Some(FractionImpl(mul_div_floor(numerator.into(), denom(U), denominator)))
		} else {
			None
		}
	}

	pub fn convert<const V: Denom>(self) -> FractionImpl<V> {
		FractionImpl(mul_div_floor(self.0, denom(V), denom(U)))
	}

	pub fn apply_exponent(&self, exp: Base10Exponent) -> Self {
		let exp = exp.0;
		if exp < 0 {
			FractionImpl(mul_div_floor(self.0, 1.into(), 10u128.pow((exp * -1) as u32)))
		} else {
			FractionImpl(mul_div_floor(self.0, 10u128.pow(exp as u32).into(), 1))
		}
	}
}

impl<const U: Denom> Add<FractionImpl<U>> for FractionImpl<U> {
	type Output = FractionImpl<U>;

	fn add(self, rhs: FractionImpl<U>) -> Self::Output {
		FractionImpl(self.0 + rhs.0)
	}
}

impl<const U: Denom> Add<FractionImpl<U>> for &FractionImpl<U> {
	type Output = FractionImpl<U>;

	fn add(self, rhs: FractionImpl<U>) -> Self::Output {
		FractionImpl(self.0 + rhs.0)
	}
}

impl<const U: Denom> Sub<FractionImpl<U>> for FractionImpl<U> {
	type Output = FractionImpl<U>;

	fn sub(self, rhs: FractionImpl<U>) -> Self::Output {
		FractionImpl(self.0 - rhs.0)
	}
}

impl<const U: Denom> Sub<FractionImpl<U>> for &FractionImpl<U> {
	type Output = FractionImpl<U>;

	fn sub(self, rhs: FractionImpl<U>) -> Self::Output {
		FractionImpl(self.0 - rhs.0)
	}
}

impl<const U: Denom, const V: Denom> Mul<FractionImpl<V>> for FractionImpl<U> {
	type Output = FractionImpl<U>;

	fn mul(self, rhs: FractionImpl<V>) -> Self::Output {
		FractionImpl(mul_div_floor(self.0, rhs.0, denom(V)))
	}
}

impl<const U: Denom, const V: Denom> Mul<FractionImpl<V>> for &FractionImpl<U> {
	type Output = FractionImpl<U>;

	fn mul(self, rhs: FractionImpl<V>) -> Self::Output {
		FractionImpl(mul_div_floor(self.0, rhs.0, denom(V)))
	}
}

#[cfg(test)]
impl<const N: u128> Arbitrary for FractionImpl<N> {
	type Parameters = ();

	fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
		use proptest::prelude::*;
		any::<[u64; 4]>().prop_map(U256).prop_map(FractionImpl)
	}

	type Strategy = impl Strategy<Value = FractionImpl<N>> + Clone + Send;
}

#[cfg(test)]
impl<const U: Denom> Div<u32> for FractionImpl<U> {
	type Output = FractionImpl<U>;

	fn div(self, rhs: u32) -> Self::Output {
		FractionImpl(mul_div_floor(self.0, 1u128.into(), rhs))
	}
}

pub type StatechainPrice = FractionImpl<{ u128::MAX }>;

/// WARNING, this assumes PRICE_FRACTIONAL_BITS = 128!
impl Into<Price> for FractionImpl<{ u128::MAX }> {
	fn into(self) -> Price {
		debug_assert_eq!(PRICE_FRACTIONAL_BITS, 128);
		self.0
	}
}

pub fn price_with_unit_to_statechain_price<const U: u128>(
	price: FractionImpl<U>,
	unit: PriceUnit,
) -> StatechainPrice {
	price
		.apply_exponent(Base10Exponent(
			unit.quote_asset.decimals() as i16 - unit.base_asset.decimals() as i16,
		))
		.convert()
}

impl<const U: Denom> PriceTrait for FractionImpl<U> {
	fn to_price_range(&self, range: BasisPoints) -> RangeInclusive<Self> {
		let delta = self * range.to_fraction();
		self + delta.clone()..=self - delta
	}
}
