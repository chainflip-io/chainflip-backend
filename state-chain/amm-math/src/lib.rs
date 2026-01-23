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

#![cfg_attr(not(feature = "std"), no_std)]

pub mod test_utilities;

pub use cf_primitives::Tick;
use cf_primitives::{Asset, BasisPoints, SignedBasisPoints, MAX_BASIS_POINTS};
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_arithmetic::traits::SaturatedConversion;
use sp_core::{U256, U512};

/// Represents an amount of an asset, in its smallest unit i.e. Ethereum has 10^-18 precision, and
/// therefore an `Amount` with the literal value of `1` would represent 10^-18 Ethereum.
pub type Amount = U256;

/// The square root of the price.
///
/// Represented as a fixed point integer with 96 fractional bits and
/// 64 integer bits (The higher bits past 96+64 th aren't used). [SqrtPrice] is always in sqrt
/// units of asset one.
#[derive(
	Clone,
	Debug,
	TypeInfo,
	Encode,
	Decode,
	MaxEncodedLen,
	Serialize,
	Deserialize,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Copy,
	Default,
)]
pub struct SqrtPrice(U256);

impl From<Price> for SqrtPrice {
	fn from(price: Price) -> Self {
		((U512::from(price.0) << Price::FRACTIONAL_BITS).integer_sqrt() >>
			(Price::FRACTIONAL_BITS - SqrtPrice::FRACTIONAL_BITS))
			.try_into()
			.unwrap_or(SqrtPrice::from_raw(U256::MAX))
	}
}

impl SqrtPrice {
	pub const FRACTIONAL_BITS: u32 = 96;

	pub fn is_valid(&self) -> bool {
		(MIN_SQRT_PRICE..=MAX_SQRT_PRICE).contains(self)
	}

	pub fn is_zero(&self) -> bool {
		self.0.is_zero()
	}

	pub fn from_amounts_bounded(quote: Amount, base: Amount) -> Self {
		assert!(!quote.is_zero() || !base.is_zero());

		if base.is_zero() {
			MAX_SQRT_PRICE
		} else {
			let unbounded_sqrt_price = SqrtPrice::try_from(
				((U512::from(quote) << 256) / U512::from(base)).integer_sqrt() >>
					(128 - Self::FRACTIONAL_BITS),
			)
			.unwrap();

			if unbounded_sqrt_price < MIN_SQRT_PRICE {
				MIN_SQRT_PRICE
			} else if unbounded_sqrt_price > MAX_SQRT_PRICE {
				MAX_SQRT_PRICE
			} else {
				unbounded_sqrt_price
			}
		}
	}

	pub fn from_raw(value: U256) -> Self {
		SqrtPrice(value)
	}

	pub fn as_raw(&self) -> U256 {
		self.0
	}

	pub fn from_tick(tick: Tick) -> Self {
		assert!(is_tick_valid(tick));

		let abs_tick = tick.unsigned_abs();

		let mut r = if abs_tick & 0x1u32 != 0 {
			U256::from(0xfffcb933bd6fad37aa2d162d1a594001u128)
		} else {
			U256::one() << 128u128
		};

		macro_rules! handle_tick_bit {
			($bit:literal, $constant:literal) => {
				/* Proof that `checked_mul` does not overflow:
					Note that the value ratio is initialized with above is such that `ratio <= (U256::one() << 128u128)`, alternatively `ratio <= (u128::MAX + 1)`
					First consider the case of applying the macro once assuming `ratio <= (U256::one() << 128u128)`:
						If ∀r ∈ U256, `r <= (U256::one() << 128u128)`, ∀C ∈ "Set of constants the macro is used with (Listed below)"
						Then `C * r <= U256::MAX` (See `debug_assertions` below)
						Therefore the `checked_mul` will not overflow
					Also note that above `(C * r >> 128u128) <= UINT128_MAX`
					Therefore if the if branch is taken ratio will be assigned a value `<= u128::MAX`
					else ratio is unchanged and remains `ratio <= u128::MAX + 1`
					Therefore as the assumption `ratio <= u128::MAX + 1` is always maintained after applying the macro,
					none of the checked_mul calls in any of the applications of the macro will overflow
				*/
				#[cfg(debug_assertions)]
				U256::checked_mul(U256::one() << 128u128, $constant.into()).unwrap();
				if abs_tick & (0x1u32 << $bit) != 0 {
					r = U256::checked_mul(r, U256::from($constant)).unwrap() >> 128u128
				}
			}
		}

		handle_tick_bit!(1, 0xfff97272373d413259a46990580e213au128);
		handle_tick_bit!(2, 0xfff2e50f5f656932ef12357cf3c7fdccu128);
		handle_tick_bit!(3, 0xffe5caca7e10e4e61c3624eaa0941cd0u128);
		handle_tick_bit!(4, 0xffcb9843d60f6159c9db58835c926644u128);
		handle_tick_bit!(5, 0xff973b41fa98c081472e6896dfb254c0u128);
		handle_tick_bit!(6, 0xff2ea16466c96a3843ec78b326b52861u128);
		handle_tick_bit!(7, 0xfe5dee046a99a2a811c461f1969c3053u128);
		handle_tick_bit!(8, 0xfcbe86c7900a88aedcffc83b479aa3a4u128);
		handle_tick_bit!(9, 0xf987a7253ac413176f2b074cf7815e54u128);
		handle_tick_bit!(10, 0xf3392b0822b70005940c7a398e4b70f3u128);
		handle_tick_bit!(11, 0xe7159475a2c29b7443b29c7fa6e889d9u128);
		handle_tick_bit!(12, 0xd097f3bdfd2022b8845ad8f792aa5825u128);
		handle_tick_bit!(13, 0xa9f746462d870fdf8a65dc1f90e061e5u128);
		handle_tick_bit!(14, 0x70d869a156d2a1b890bb3df62baf32f7u128);
		handle_tick_bit!(15, 0x31be135f97d08fd981231505542fcfa6u128);
		handle_tick_bit!(16, 0x9aa508b5b7a84e1c677de54f3e99bc9u128);
		handle_tick_bit!(17, 0x5d6af8dedb81196699c329225ee604u128);
		handle_tick_bit!(18, 0x2216e584f5fa1ea926041bedfe98u128);
		handle_tick_bit!(19, 0x48a170391f7dc42444e8fa2u128);
		// Note due to MIN_TICK and MAX_TICK bounds, past the 20th bit abs_tick is all zeros

		/* Proof that r is never zero (therefore avoiding the divide by zero case here):
			We can think of an application of the `handle_tick_bit` macro as increasing the index I of r's MSB/`r.ilog2()` (mul by constant), and then decreasing it by 128 (the right shift).

			Note the increase in I caused by the constant mul will be at least constant.ilog2().

			Also note each application of `handle_tick_bit` decreases (if the if branch is entered) or else maintains r's value as all the constants are less than 2^128.

			Therefore the largest decrease would be caused if all the macros application's if branches where entered.

			So we assuming all if branches are entered, after all the applications `I` would be at least I_initial + bigsum(constant.ilog2()) - 19*128.

			The test `r_non_zero` checks with value is >= 0, therefore imply the smallest value r could have is more than 0.
		*/
		let sqrt_price_q32f128 = if tick > 0 { U256::MAX / r } else { r };

		// we round up in the division so tick_at_sqrt_price of the output price is always
		// consistent
		SqrtPrice(
			(sqrt_price_q32f128 >> 32u128) +
				if sqrt_price_q32f128.low_u32() == 0 { U256::zero() } else { U256::one() },
		)
	}

	/// Calculates the greatest tick value such that `sqrt_price_at_tick(tick) <= sqrt_price`
	pub fn to_tick(self) -> Tick {
		assert!(self.is_valid());

		let sqrt_price_q64f128 = self.0 << 32u128;

		let (integer_log_2, mantissa) = {
			let mut _bits_remaining = sqrt_price_q64f128;
			let mut most_significant_bit = 0u8;

			// rustfmt chokes when formatting this macro.
			// See: https://github.com/rust-lang/rustfmt/issues/5404
			#[rustfmt::skip]
			macro_rules! add_integer_bit {
				($bit:literal, $lower_bits_mask:literal) => {
					if _bits_remaining > U256::from($lower_bits_mask) {
						most_significant_bit |= $bit;
						_bits_remaining >>= $bit;
					}
				};
			}

			add_integer_bit!(128u8, 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFu128);
			add_integer_bit!(64u8, 0xFFFFFFFFFFFFFFFFu128);
			add_integer_bit!(32u8, 0xFFFFFFFFu128);
			add_integer_bit!(16u8, 0xFFFFu128);
			add_integer_bit!(8u8, 0xFFu128);
			add_integer_bit!(4u8, 0xFu128);
			add_integer_bit!(2u8, 0x3u128);
			add_integer_bit!(1u8, 0x1u128);

			(
				// most_significant_bit is the log2 of sqrt_price_q64f128 as an integer. This
				// converts most_significant_bit to the integer log2 of sqrt_price_q64f128 as an
				// q64f128
				((most_significant_bit as i16) + (-128i16)) as i8,
				// Calculate mantissa of sqrt_price_q64f128.
				if most_significant_bit >= 128u8 {
					// The bits we possibly drop when right shifting don't contribute to the log2
					// above the 14th fractional bit.
					sqrt_price_q64f128 >> (most_significant_bit - 127u8)
				} else {
					sqrt_price_q64f128 << (127u8 - most_significant_bit)
				}
				.as_u128(), // Conversion to u128 is safe as top 128 bits are always zero
			)
		};

		let log_2_q63f64 = {
			let mut log_2_q63f64 = (integer_log_2 as i128) << 64u8;
			let mut _mantissa = mantissa;

			// rustfmt chokes when formatting this macro.
			// See: https://github.com/rust-lang/rustfmt/issues/5404
			#[rustfmt::skip]
			macro_rules! add_fractional_bit {
				($bit:literal) => {
					// Note squaring a number doubles its log
					let mantissa_sq =
						(U256::checked_mul(_mantissa.into(), _mantissa.into()).unwrap() >> 127u8);
					_mantissa = if mantissa_sq.bit(128) {
						// is the 129th bit set, all higher bits must be zero due to 127 right bit
						// shift
						log_2_q63f64 |= 1i128 << $bit;
						(mantissa_sq >> 1u8).as_u128()
					} else {
						mantissa_sq.as_u128()
					}
				};
			}

			add_fractional_bit!(63u8);
			add_fractional_bit!(62u8);
			add_fractional_bit!(61u8);
			add_fractional_bit!(60u8);
			add_fractional_bit!(59u8);
			add_fractional_bit!(58u8);
			add_fractional_bit!(57u8);
			add_fractional_bit!(56u8);
			add_fractional_bit!(55u8);
			add_fractional_bit!(54u8);
			add_fractional_bit!(53u8);
			add_fractional_bit!(52u8);
			add_fractional_bit!(51u8);
			add_fractional_bit!(50u8);

			// We don't need more precision than (63..50) = 14 bits

			log_2_q63f64
		};

		// Note we don't have a I256 type so I have to handle the negative mul case manually
		let log_sqrt10001_q127f128 = U256::overflowing_mul(
			if log_2_q63f64 < 0 {
				(U256::from(u128::MAX) << 128u8) | U256::from(log_2_q63f64 as u128)
			} else {
				U256::from(log_2_q63f64 as u128)
			},
			U256::from(255738958999603826347141u128),
		)
		.0;

		let tick_low = (U256::overflowing_sub(
			log_sqrt10001_q127f128,
			U256::from(3402992956809132418596140100660247210u128),
		)
		.0 >> 128u8)
			.as_u128() as Tick; // Add Checks
		let tick_high = (U256::overflowing_add(
			log_sqrt10001_q127f128,
			U256::from(291339464771989622907027621153398088495u128),
		)
		.0 >> 128u8)
			.as_u128() as Tick; // Add Checks

		if tick_low == tick_high {
			tick_low
		} else if Self::from_tick(tick_high) <= self {
			tick_high
		} else {
			tick_low
		}
	}
}

impl TryFrom<U512> for SqrtPrice {
	type Error = ();

	fn try_from(value: U512) -> Result<Self, Self::Error> {
		U256::try_from(value).map(SqrtPrice).map_err(|_| ())
	}
}

pub fn mul_div_floor<C: Into<U512>>(a: U256, b: U256, c: C) -> U256 {
	mul_div_floor_checked(a, b, c).unwrap()
}

pub fn mul_div_floor_checked<C: Into<U512>>(a: U256, b: U256, c: C) -> Option<U256> {
	let c: U512 = c.into();
	(U256::full_mul(a, b) / c).try_into().ok()
}

pub fn mul_div_ceil<C: Into<U512>>(a: U256, b: U256, c: C) -> U256 {
	mul_div(a, b, c).1
}

pub fn mul_div<C: Into<U512>>(a: U256, b: U256, c: C) -> (U256, U256) {
	let c: U512 = c.into();

	let (d, m) = U512::div_mod(U256::full_mul(a, b), c);

	(
		d.try_into().unwrap(),
		if m > U512::from(0) {
			// cannot overflow as for m > 0, c must be > 1, and as (a*b) < U512::MAX, therefore
			// a*b/c < U512::MAX
			d + 1
		} else {
			d
		}
		.try_into()
		.unwrap(),
	)
}

#[derive(
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Debug,
	Clone,
	Copy,
	Default,
	Encode,
	Decode,
	Serialize,
	Deserialize,
	TypeInfo,
	MaxEncodedLen,
)]
/// This is the ratio of equivalently valued amounts of base and quote assets.
///
/// The price is always measured in amount of quote asset per unit of base asset. Therefore as base
/// asset becomes more valuable relative to quote asset the prices literal value goes up, and vice
/// versa. This ratio is represented as a fixed point number with `PRICE_FRACTIONAL_BITS` fractional
/// bits.
pub struct Price(U256);

impl From<SqrtPrice> for Price {
	fn from(sqrt_price: SqrtPrice) -> Self {
		assert!(sqrt_price.is_valid());

		// Note the value here cannot ever be zero as MIN_SQRT_PRICE has its 33th bit set, so
		// sqrt_price will always include a bit pass the 64th bit that is set, so when we shift
		// down below that set bit will not be removed.
		Price(mul_div_floor(
			sqrt_price.0,
			sqrt_price.0,
			U256::one() << (2 * SqrtPrice::FRACTIONAL_BITS - Price::FRACTIONAL_BITS),
		))
	}
}

impl Price {
	pub const FRACTIONAL_BITS: u32 = 128;

	pub fn from_raw(value: U256) -> Self {
		Price(value)
	}

	pub fn as_raw(self) -> U256 {
		self.0
	}

	pub const fn zero() -> Self {
		Price(U256::zero())
	}

	pub fn is_zero(&self) -> bool {
		self.0.is_zero()
	}

	/// Converts a `tick` to a `price`. Will return `None` for ticks outside MIN_TICK..=MAX_TICK
	///
	/// This function never panics.
	pub fn from_tick(tick: Tick) -> Option<Self> {
		if is_tick_valid(tick) {
			Some(SqrtPrice::from_tick(tick).into())
		} else {
			None
		}
	}

	pub fn from_amounts_bounded(quote: Amount, base: Amount) -> Self {
		SqrtPrice::from_amounts_bounded(quote, base).into()
	}

	pub fn from_amounts(quote: Amount, base: Amount) -> Self {
		Price(mul_div_floor(quote, U256::one() << Self::FRACTIONAL_BITS, base))
	}

	/// The price obtained by selling `input` amount of asset to receive `output` amount of asset.
	///
	/// Higher sell price is better for the seller (more output for the same input).
	pub fn sell_price(input: Amount, output: Amount) -> Self {
		Self::from_amounts(output, input)
	}

	/// The price of buying `output` amount of asset with `input` amount of asset.
	///
	/// Higher buy price is worse for the buyer (less output for the same input).
	pub fn buy_price(input: Amount, output: Amount) -> Self {
		Self::from_amounts(input, output)
	}

	/// Compute the price of asset 1 (self) in terms of asset 2 (given).
	/// Both prices must have the same quote asset (eg. USD).
	pub fn relative_to(self, price: Price) -> Self {
		Price(mul_div_floor(self.0, U256::one() << Self::FRACTIONAL_BITS, price.0))
	}

	pub fn output_amount_floor<I: Into<U256>>(self, input: I) -> Amount {
		mul_div_floor(input.into(), self.0, U256::one() << Self::FRACTIONAL_BITS)
	}

	pub fn output_amount_ceil<I: Into<U256>>(self, input: I) -> Amount {
		mul_div_ceil(input.into(), self.0, U256::one() << Self::FRACTIONAL_BITS)
	}

	pub fn input_amount_floor<I: Into<U256>>(self, output: I) -> Amount {
		mul_div_floor(output.into(), U256::one() << Self::FRACTIONAL_BITS, self.0)
	}

	pub fn input_amount_ceil<I: Into<U256>>(self, output: I) -> Amount {
		mul_div_ceil(output.into(), U256::one() << Self::FRACTIONAL_BITS, self.0)
	}

	/// Given price of asset 1 in terms of asset 2, compute the price of asset 2 in terms of asset 1
	pub fn invert(self) -> Self {
		Price(mul_div_floor(
			U256::one() << Self::FRACTIONAL_BITS,
			U256::one() << Self::FRACTIONAL_BITS,
			self.0,
		))
	}

	/// Converts a `price` to a `tick`. Will return `None` if the price is too high or low to be
	/// represented by a valid tick i.e. one inside MIN_TICK..=MAX_TICK.
	///
	/// This function never panics.
	pub fn into_tick(self) -> Option<Tick> {
		let sqrt_price = SqrtPrice::from(self);
		if sqrt_price.is_valid() {
			Some(sqrt_price.to_tick())
		} else {
			None
		}
	}

	pub fn at_tick_zero() -> Self {
		Self::from_tick(0).unwrap()
	}

	/// Get a price from a USD fine amount. The quote asset will be USD.
	#[cfg(any(feature = "runtime-benchmarks", feature = "test", test))]
	pub fn from_usd_fine_amount(price_usd: cf_primitives::AssetAmount) -> Self {
		Self(U256::from(price_usd) << Self::FRACTIONAL_BITS)
	}

	/// Get the price of an asset given its USD cents amount. The price is automatically scaled to
	/// the asset's decimals. The quote asset will be USD.
	pub fn from_usd_cents(asset: Asset, cents_amount: u32) -> Self {
		if cents_amount == 0 {
			return Self(U256::zero());
		}
		Self(
			(U256::from(cents_amount) << Self::FRACTIONAL_BITS) /
				10u128.pow(asset.decimals() + 2 - Asset::Usdc.decimals()),
		)
	}

	/// Get the price of an asset given its dollar amount. The price is automatically scaled to the
	/// asset's decimals. The quote asset will be USD.
	#[cfg(any(feature = "runtime-benchmarks", feature = "test", test))]
	pub fn from_usd(asset: Asset, dollar_amount: u32) -> Self {
		Self::from_usd_cents(asset, dollar_amount * 100)
	}

	pub fn adjust_by_bps(self, bps: BasisPoints, increase: bool) -> Self {
		let adjusted_bps = if increase { MAX_BASIS_POINTS + bps } else { MAX_BASIS_POINTS - bps };
		Self(mul_div_floor(self.0, U256::from(adjusted_bps), MAX_BASIS_POINTS))
	}
	/// Calculates the basis points difference from some other price to this one, assuming they
	/// are both prices of the same base/quote pair.
	///
	/// The `from` implies that if the other price is lower than self, the result will be positive,
	/// and if the other price is higher than self, the result will be negative.
	pub fn bps_difference_from(&self, other_price: &Price) -> SignedBasisPoints {
		let abs_diff = self.0.abs_diff(other_price.0);
		let abs_diff_bps = mul_div_ceil(abs_diff, MAX_BASIS_POINTS.into(), other_price.0);
		let sign = if self.0 < other_price.0 { -1 } else { 1 };
		abs_diff_bps.saturated_into::<SignedBasisPoints>() * sign
	}
}

/// The minimum tick that may be passed to `sqrt_price_at_tick` computed from log base 1.0001 of
/// 2**-128
pub const MIN_TICK: Tick = -887272;
/// The maximum tick that may be passed to `sqrt_price_at_tick` computed from log base 1.0001 of
/// 2**128
pub const MAX_TICK: Tick = -MIN_TICK;
/// The minimum value that can be returned from `sqrt_price_at_tick`. Equivalent to
/// `sqrt_price_at_tick(MIN_TICK)`
pub const MIN_SQRT_PRICE: SqrtPrice = SqrtPrice(U256([0x1000276a3u64, 0x0, 0x0, 0x0]));
/// The maximum value that can be returned from `sqrt_price_at_tick`. Equivalent to
/// `sqrt_price_at_tick(MAX_TICK)`.
pub const MAX_SQRT_PRICE: SqrtPrice =
	SqrtPrice(U256([0x5d951d5263988d26u64, 0xefd1fc6a50648849u64, 0xfffd8963u64, 0x0u64]));

pub fn is_tick_valid(tick: Tick) -> bool {
	(MIN_TICK..=MAX_TICK).contains(&tick)
}

#[derive(
	Debug,
	Clone,
	PartialEq,
	Eq,
	Encode,
	Decode,
	TypeInfo,
	Serialize,
	Deserialize,
	MaxEncodedLen,
	PartialOrd,
	Ord,
	Copy,
	Default,
)]
pub struct PriceLimits {
	pub min_price: Price,
	pub max_oracle_price_slippage: Option<BasisPoints>,
}

#[cfg(test)]
mod test {
	use super::*;

	#[cfg(feature = "slow-tests")]
	use rand::SeedableRng;

	#[cfg(feature = "slow-tests")]
	use crate::test_utilities::rng_u256_inclusive_bound;

	#[cfg(feature = "slow-tests")]
	#[test]
	fn test_sqrt_price() {
		let mut rng: rand::rngs::StdRng = rand::rngs::StdRng::from_seed([0; 32]);

		for _i in 0..10000000 {
			assert!(SqrtPrice::from_amounts_bounded(
				rng_u256_inclusive_bound(&mut rng, Amount::one()..=Amount::MAX),
				rng_u256_inclusive_bound(&mut rng, Amount::one()..=Amount::MAX),
			)
			.is_valid());
		}
	}

	#[test]
	fn test_mul_div_floor() {
		assert_eq!(mul_div_floor(U256::from(1), U256::from(1), 1), 1.into());
		assert_eq!(mul_div_floor(U256::from(1), U256::from(1), 2), 0.into());
		assert_eq!(mul_div_floor(U256::from(1), U256::from(2), 1), 2.into());
		assert_eq!(mul_div_floor(U256::from(1), U256::from(2), 2), 1.into());
		assert_eq!(mul_div_floor(U256::from(1), U256::from(2), 3), 0.into());
		assert_eq!(mul_div_floor(U256::from(1), U256::from(3), 2), 1.into());
		assert_eq!(mul_div_floor(U256::from(1), U256::from(3), 3), 1.into());
		assert_eq!(mul_div_floor(U256::from(1), U256::from(3), 4), 0.into());
		assert_eq!(mul_div_floor(U256::from(1), U256::from(4), 3), 1.into());
		assert_eq!(mul_div_floor(U256::from(1), U256::from(4), 4), 1.into());
		assert_eq!(mul_div_floor(U256::from(1), U256::from(4), 5), 0.into());
		assert_eq!(mul_div_floor(U256::from(1), U256::from(5), 4), 1.into());
		assert_eq!(mul_div_floor(U256::from(1), U256::from(5), 5), 1.into());
		assert_eq!(mul_div_floor(U256::from(1), U256::from(5), 6), 0.into());

		assert_eq!(mul_div_floor(U256::from(2), U256::from(1), 2), 1.into());
		assert_eq!(mul_div_floor(U256::from(2), U256::from(1), 3), 0.into());
		assert_eq!(mul_div_floor(U256::from(3), U256::from(1), 2), 1.into());
		assert_eq!(mul_div_floor(U256::from(3), U256::from(1), 3), 1.into());
		assert_eq!(mul_div_floor(U256::from(3), U256::from(1), 4), 0.into());
		assert_eq!(mul_div_floor(U256::from(4), U256::from(1), 3), 1.into());
		assert_eq!(mul_div_floor(U256::from(4), U256::from(1), 4), 1.into());
		assert_eq!(mul_div_floor(U256::from(4), U256::from(1), 5), 0.into());
		assert_eq!(mul_div_floor(U256::from(5), U256::from(1), 4), 1.into());
		assert_eq!(mul_div_floor(U256::from(5), U256::from(1), 5), 1.into());
		assert_eq!(mul_div_floor(U256::from(5), U256::from(1), 6), 0.into());

		assert_eq!(mul_div_floor(U256::from(2), U256::from(1), 1), 2.into());
		assert_eq!(mul_div_floor(U256::from(2), U256::from(1), 2), 1.into());

		assert_eq!(mul_div_floor(U256::MAX, U256::MAX, U256::MAX), U256::MAX);
		assert_eq!(mul_div_floor(U256::MAX, U256::MAX - 1, U256::MAX), U256::MAX - 1);
	}

	#[test]
	fn test_mul_div() {
		assert_eq!(mul_div(U256::MAX, U256::MAX, U256::MAX), (U256::MAX, U256::MAX));
		assert_eq!(mul_div(U256::MAX, U256::MAX - 1, U256::MAX), (U256::MAX - 1, U256::MAX - 1));
		assert_eq!(mul_div(2.into(), 2.into(), 3), (1.into(), 2.into()));
		assert_eq!(mul_div(2.into(), 2.into(), 4), (1.into(), 1.into()));
		assert_eq!(mul_div(2.into(), 2.into(), 5), (0.into(), 1.into()));
		assert_eq!(mul_div(2.into(), 2.into(), 6), (0.into(), 1.into()));
	}

	#[cfg(feature = "slow-tests")]
	#[test]
	fn test_conversion_sqrt_price_back_and_forth() {
		for tick in MIN_TICK..=MAX_TICK {
			assert_eq!(tick, SqrtPrice::from_tick(tick).to_tick());
		}
	}

	#[test]
	fn test_sqrt_price_at_tick() {
		fn test_tick(tick: Tick, expected_sqrt_price_string: &str) {
			assert_eq!(
				SqrtPrice::from_tick(tick).0,
				U256::from_dec_str(expected_sqrt_price_string).unwrap()
			);
		}

		assert_eq!(SqrtPrice::from_tick(MIN_TICK), MIN_SQRT_PRICE);
		test_tick(-738203, "7409801140451");
		test_tick(-500000, "1101692437043807371");
		test_tick(-250000, "295440463448801648376846");
		test_tick(-150000, "43836292794701720435367485");
		test_tick(-50000, "6504256538020985011912221507");
		test_tick(-5000, "61703726247759831737814779831");
		test_tick(-4000, "64867181785621769311890333195");
		test_tick(-3000, "68192822843687888778582228483");
		test_tick(-2500, "69919044979842180277688105136");
		test_tick(-1000, "75364347830767020784054125655");
		test_tick(-500, "77272108795590369356373805297");
		test_tick(-250, "78244023372248365697264290337");
		test_tick(-100, "78833030112140176575862854579");
		test_tick(-50, "79030349367926598376800521322");
		test_tick(50, "79426470787362580746886972461");
		test_tick(100, "79625275426524748796330556128");
		test_tick(250, "80224679980005306637834519095");
		test_tick(500, "81233731461783161732293370115");
		test_tick(1000, "83290069058676223003182343270");
		test_tick(2500, "89776708723587163891445672585");
		test_tick(3000, "92049301871182272007977902845");
		test_tick(4000, "96768528593268422080558758223");
		test_tick(5000, "101729702841318637793976746270");
		test_tick(50000, "965075977353221155028623082916");
		test_tick(150000, "143194173941309278083010301478497");
		test_tick(250000, "21246587762933397357449903968194344");
		test_tick(500000, "5697689776495288729098254600827762987878");
		test_tick(738203, "847134979253254120489401328389043031315994541");
		assert_eq!(SqrtPrice::from_tick(MAX_TICK), MAX_SQRT_PRICE);
	}

	#[test]
	fn test_tick_at_sqrt_price() {
		fn test_sqrt_price(sqrt_price_string: &str, expected_tick: Tick) {
			assert_eq!(
				SqrtPrice(U256::from_dec_str(sqrt_price_string).unwrap()).to_tick(),
				expected_tick
			);
		}

		assert_eq!(MIN_SQRT_PRICE.to_tick(), MIN_TICK);
		test_sqrt_price("79228162514264337593543", -276325);
		test_sqrt_price("79228162514264337593543950", -138163);
		test_sqrt_price("9903520314283042199192993792", -41591);
		test_sqrt_price("28011385487393069959365969113", -20796);
		test_sqrt_price("56022770974786139918731938227", -6932);
		test_sqrt_price("79228162514264337593543950336", 0);
		test_sqrt_price("112045541949572279837463876454", 6931);
		test_sqrt_price("224091083899144559674927752909", 20795);
		test_sqrt_price("633825300114114700748351602688", 41590);
		test_sqrt_price("79228162514264337593543950336000", 138162);
		test_sqrt_price("79228162514264337593543950336000000", 276324);
		assert_eq!((SqrtPrice(MAX_SQRT_PRICE.0 - 1)).to_tick(), MAX_TICK - 1);
		assert_eq!(MAX_SQRT_PRICE.to_tick(), MAX_TICK);
	}

	#[test]
	fn test_relative_price() {
		fn relative_price(price_1: U256, price_2: U256) -> U256 {
			Price(price_1).relative_to(Price(price_2)).0
		}

		assert_eq!(
			relative_price(U256::from(1), U256::from(1)),
			U256::one() << Price::FRACTIONAL_BITS
		);
		assert_eq!(
			relative_price(U256::from(2), U256::from(1)),
			(U256::one() << Price::FRACTIONAL_BITS) * 2
		);
		assert_eq!(
			relative_price(U256::from(1), U256::from(2)),
			(U256::one() << Price::FRACTIONAL_BITS) / 2
		);
		assert_eq!(
			relative_price(U256::from(1), U256::from(3)),
			(U256::one() << Price::FRACTIONAL_BITS) / 3
		);
		assert_eq!(
			relative_price(U256::from(2), U256::from(3)),
			(U256::one() << Price::FRACTIONAL_BITS) * 2 / 3
		);
		// Manually calculated value
		assert_eq!(
			relative_price(
				U256::from_dec_str("1234512345123451234512345123451234512345").unwrap(),
				U256::from_dec_str("4567845678456784567845678456784567845678").unwrap()
			),
			U256::from_dec_str("91965187171920516035188920897262983721").unwrap()
		);
	}

	#[test]
	fn test_price_from_sqrt_price() {
		assert_eq!(
			Price::from(SqrtPrice::from_raw(U256::from(1) << 96)),
			Price::from_raw(U256::from(1) << Price::FRACTIONAL_BITS)
		);
		assert!(Price::from(MIN_SQRT_PRICE) < Price::from(MAX_SQRT_PRICE));
	}

	#[test]
	fn test_asset_decimals() {
		for asset in Asset::all() {
			assert!(
				asset.decimals() >= Asset::Usdc.decimals(),
				"No asset should have less decimals than USDC or the from_usd_cents function will fail. Asset: {:?}",
				asset
			);
		}
	}

	#[test]
	fn test_price_invert() {
		let price = Price::from_usd_cents(Asset::Eth, 12345);

		assert_eq!(price.invert().invert(), price);
		assert_eq!(
			price.output_amount_floor(U256::from(123 * 10u128.pow(18))),
			price.invert().input_amount_floor(U256::from(123 * 10u128.pow(18)))
		);
	}

	#[test]
	fn test_price_bps_difference() {
		let ref_price = Price::from_usd_fine_amount(100000);
		let price_1 = Price::from_usd_fine_amount(95000); // ok
		let price_1_1 = Price::from_usd_fine_amount(94999); // over the limit
		let price_2 = Price::from_usd_fine_amount(105000); // ok
		let price_2_1 = Price::from_usd_fine_amount(105001); // over the limit
		assert_eq!(price_1.bps_difference_from(&ref_price), -500);
		assert_eq!(price_1_1.bps_difference_from(&ref_price), -501);
		assert_eq!(price_2.bps_difference_from(&ref_price), 500);
		assert_eq!(price_2_1.bps_difference_from(&ref_price), 501);
	}
}
