use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_core::{U256, U512};

pub const ONE_IN_HUNDREDTH_PIPS: u32 = 1_000_000;
pub const MAX_LP_FEE: u32 = ONE_IN_HUNDREDTH_PIPS / 2;

/// Represents an amount of an asset, in its smallest unit i.e. Ethereum has 10^-18 precision, and
/// therefore an `Amount` with the literal value of `1` would represent 10^-18 Ethereum.
pub type Amount = U256;
/// The `log1.0001(price)` rounded to the nearest integer. Note [Price] is always
/// in units of asset One.
pub type Tick = i32;
/// The square root of the price, represented as a fixed point integer with 96 fractional bits and
/// 64 integer bits (The higher bits past 96+64 th aren't used). [SqrtPriceQ64F96] is always in sqrt
/// units of asset one.
pub type SqrtPriceQ64F96 = U256;
/// The number of fractional bits used by `SqrtPriceQ64F96`.
pub const SQRT_PRICE_FRACTIONAL_BITS: u32 = 96;

#[derive(Debug)]
pub enum SetFeesError {
	/// Fee must be between 0 - 50%
	InvalidFeeAmount,
}

#[derive(
	Debug,
	Clone,
	Copy,
	PartialEq,
	Eq,
	Encode,
	Decode,
	MaxEncodedLen,
	TypeInfo,
	Deserialize,
	Serialize,
	Hash,
)]
#[serde(rename_all = "snake_case")]
pub enum Side {
	Buy,
	Sell,
}
impl Side {
	pub fn to_sold_pair(&self) -> Pairs {
		match self {
			Side::Buy => Pairs::Quote,
			Side::Sell => Pairs::Base,
		}
	}
}
impl core::ops::Not for Side {
	type Output = Self;

	fn not(self) -> Self::Output {
		match self {
			Side::Sell => Side::Buy,
			Side::Buy => Side::Sell,
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub enum Pairs {
	Base,
	Quote,
}

impl core::ops::Not for Pairs {
	type Output = Self;

	fn not(self) -> Self::Output {
		match self {
			Pairs::Base => Pairs::Quote,
			Pairs::Quote => Pairs::Base,
		}
	}
}

impl Pairs {
	pub fn sell_order(&self) -> Side {
		match self {
			Pairs::Base => Side::Sell,
			Pairs::Quote => Side::Buy,
		}
	}
}

#[derive(
	Copy,
	Clone,
	Default,
	Debug,
	TypeInfo,
	PartialEq,
	Eq,
	Hash,
	Encode,
	Decode,
	MaxEncodedLen,
	Serialize,
	Deserialize,
)]
pub struct PoolPairsMap<T> {
	pub base: T,
	pub quote: T,
}

impl<T> PoolPairsMap<T> {
	pub fn from_array(array: [T; 2]) -> Self {
		let [base, quote] = array;
		Self { base, quote }
	}

	pub fn map<R, F: FnMut(T) -> R>(self, mut f: F) -> PoolPairsMap<R> {
		PoolPairsMap { base: f(self.base), quote: f(self.quote) }
	}

	pub fn try_map<R, E, F: FnMut(T) -> Result<R, E>>(
		self,
		mut f: F,
	) -> Result<PoolPairsMap<R>, E> {
		Ok(PoolPairsMap { base: f(self.base)?, quote: f(self.quote)? })
	}

	pub fn try_map_with_pair<R, E>(
		self,
		mut f: impl FnMut(Pairs, T) -> Result<R, E>,
	) -> Result<PoolPairsMap<R>, E> {
		Ok(PoolPairsMap { base: f(Pairs::Base, self.base)?, quote: f(Pairs::Quote, self.quote)? })
	}

	pub fn as_ref(&self) -> PoolPairsMap<&T> {
		PoolPairsMap { base: &self.base, quote: &self.quote }
	}

	pub fn as_mut(&mut self) -> PoolPairsMap<&mut T> {
		PoolPairsMap { base: &mut self.base, quote: &mut self.quote }
	}

	pub fn zip<S>(self, other: PoolPairsMap<S>) -> PoolPairsMap<(T, S)> {
		PoolPairsMap { base: (self.base, other.base), quote: (self.quote, other.quote) }
	}

	pub fn map_with_pair<R, F: FnMut(Pairs, T) -> R>(self, mut f: F) -> PoolPairsMap<R> {
		PoolPairsMap { base: f(Pairs::Base, self.base), quote: f(Pairs::Quote, self.quote) }
	}
}
impl<T> IntoIterator for PoolPairsMap<T> {
	type Item = (Pairs, T);

	type IntoIter = core::array::IntoIter<(Pairs, T), 2>;

	fn into_iter(self) -> Self::IntoIter {
		[(Pairs::Base, self.base), (Pairs::Quote, self.quote)].into_iter()
	}
}
impl<T> core::ops::Index<Pairs> for PoolPairsMap<T> {
	type Output = T;
	fn index(&self, side: Pairs) -> &T {
		match side {
			Pairs::Base => &self.base,
			Pairs::Quote => &self.quote,
		}
	}
}
impl<T> core::ops::IndexMut<Pairs> for PoolPairsMap<T> {
	fn index_mut(&mut self, side: Pairs) -> &mut T {
		match side {
			Pairs::Base => &mut self.base,
			Pairs::Quote => &mut self.quote,
		}
	}
}
impl<T: core::ops::Add<R>, R> core::ops::Add<PoolPairsMap<R>> for PoolPairsMap<T> {
	type Output = PoolPairsMap<<T as core::ops::Add<R>>::Output>;
	fn add(self, rhs: PoolPairsMap<R>) -> Self::Output {
		PoolPairsMap { base: self.base + rhs.base, quote: self.quote + rhs.quote }
	}
}

pub fn mul_div_floor<C: Into<U512>>(a: U256, b: U256, c: C) -> U256 {
	let c: U512 = c.into();
	(U256::full_mul(a, b) / c).try_into().unwrap()
}

pub fn mul_div_ceil<C: Into<U512>>(a: U256, b: U256, c: C) -> U256 {
	mul_div(a, b, c).1
}

pub(super) fn mul_div<C: Into<U512>>(a: U256, b: U256, c: C) -> (U256, U256) {
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

pub fn bounded_sqrt_price(quote: Amount, base: Amount) -> SqrtPriceQ64F96 {
	assert!(!quote.is_zero() || !base.is_zero());

	if base.is_zero() {
		MAX_SQRT_PRICE
	} else {
		let unbounded_sqrt_price = SqrtPriceQ64F96::try_from(
			((U512::from(quote) << 256) / U512::from(base)).integer_sqrt() >>
				(128 - SQRT_PRICE_FRACTIONAL_BITS),
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

/// A marker type to represent a swap that buys asset Quote, and sells asset Base
pub(super) struct BaseToQuote {}
/// A marker type to represent a swap that buys asset Base, and sells asset Quote
pub(super) struct QuoteToBase {}

pub(super) trait SwapDirection {
	/// The asset this type of swap sells, i.e. the asset the swapper provides
	const INPUT_SIDE: Pairs;

	/// The worst price in this swap direction
	const WORST_SQRT_PRICE: SqrtPriceQ64F96;

	/// Determines if a given sqrt_price is more than another for this direction of swap.
	fn sqrt_price_op_more_than(
		sqrt_price: SqrtPriceQ64F96,
		sqrt_price_other: SqrtPriceQ64F96,
	) -> bool;

	/// Increases a valid sqrt_price by a specified number of ticks
	fn increase_sqrt_price(sqrt_price: SqrtPriceQ64F96, delta: Tick) -> SqrtPriceQ64F96;

	/// Returns the equivalent saturated amount in the output asset to a given amount of the input
	/// asset at a specific tick, will return None iff the tick is invalid
	fn input_to_output_amount_floor(amount: Amount, tick: Tick) -> Option<Amount>;
}
impl SwapDirection for BaseToQuote {
	const INPUT_SIDE: Pairs = Pairs::Base;

	const WORST_SQRT_PRICE: SqrtPriceQ64F96 = MIN_SQRT_PRICE;

	fn sqrt_price_op_more_than(
		sqrt_price: SqrtPriceQ64F96,
		sqrt_price_other: SqrtPriceQ64F96,
	) -> bool {
		sqrt_price < sqrt_price_other
	}

	fn increase_sqrt_price(sqrt_price: SqrtPriceQ64F96, delta: Tick) -> SqrtPriceQ64F96 {
		sqrt_price_at_tick(tick_at_sqrt_price(sqrt_price).saturating_sub(delta).max(MIN_TICK))
	}

	fn input_to_output_amount_floor(amount: Amount, tick: Tick) -> Option<Amount> {
		if is_tick_valid(tick) {
			Some(
				(U256::full_mul(amount, sqrt_price_to_price(sqrt_price_at_tick(tick))) /
					(U256::one() << PRICE_FRACTIONAL_BITS))
					.try_into()
					.unwrap_or(U256::MAX),
			)
		} else {
			None
		}
	}
}
impl SwapDirection for QuoteToBase {
	const INPUT_SIDE: Pairs = Pairs::Quote;

	const WORST_SQRT_PRICE: SqrtPriceQ64F96 = MAX_SQRT_PRICE;

	fn sqrt_price_op_more_than(
		sqrt_price: SqrtPriceQ64F96,
		sqrt_price_other: SqrtPriceQ64F96,
	) -> bool {
		sqrt_price > sqrt_price_other
	}

	fn increase_sqrt_price(sqrt_price: SqrtPriceQ64F96, delta: Tick) -> SqrtPriceQ64F96 {
		let tick = tick_at_sqrt_price(sqrt_price);
		sqrt_price_at_tick(
			if sqrt_price == sqrt_price_at_tick(tick) { tick } else { tick + 1 }
				.saturating_add(delta)
				.min(MAX_TICK),
		)
	}

	fn input_to_output_amount_floor(amount: Amount, tick: Tick) -> Option<Amount> {
		if is_tick_valid(tick) {
			Some(
				(U256::full_mul(amount, U256::one() << PRICE_FRACTIONAL_BITS) /
					sqrt_price_to_price(sqrt_price_at_tick(tick)))
				.try_into()
				.unwrap_or(U256::MAX),
			)
		} else {
			None
		}
	}
}

// TODO: Consider increasing Price to U512 or switch to a f64 (f64 would only be for the external
// price representation), as at low ticks the precision in the price is VERY LOW, but this does not
// cause any problems for the AMM code in terms of correctness
/// This is the ratio of equivalently valued amounts of asset One and asset Zero. The price is
/// always measured in amount of asset One per unit of asset Zero. Therefore as asset zero becomes
/// more valuable relative to asset one the price's literal value goes up, and vice versa. This
/// ratio is represented as a fixed point number with `PRICE_FRACTIONAL_BITS` fractional bits.
pub type Price = U256;
pub const PRICE_FRACTIONAL_BITS: u32 = 128;

/// Converts from a [SqrtPriceQ64F96] to a [Price].
///
/// Will panic for `sqrt_price`'s outside `MIN_SQRT_PRICE..=MAX_SQRT_PRICE`
pub(super) fn sqrt_price_to_price(sqrt_price: SqrtPriceQ64F96) -> Price {
	assert!(is_sqrt_price_valid(sqrt_price));

	// Note the value here cannot ever be zero as MIN_SQRT_PRICE has its 33th bit set, so sqrt_price
	// will always include a bit pass the 64th bit that is set, so when we shift down below that set
	// bit will not be removed.
	mul_div_floor(
		sqrt_price,
		sqrt_price,
		SqrtPriceQ64F96::one() << (2 * SQRT_PRICE_FRACTIONAL_BITS - PRICE_FRACTIONAL_BITS),
	)
}

/// Converts from a `price` to a `sqrt_price`
///
/// This function never panics.
pub(super) fn price_to_sqrt_price(price: Price) -> SqrtPriceQ64F96 {
	((U512::from(price) << PRICE_FRACTIONAL_BITS).integer_sqrt() >>
		(PRICE_FRACTIONAL_BITS - SQRT_PRICE_FRACTIONAL_BITS))
		.try_into()
		.unwrap_or(SqrtPriceQ64F96::MAX)
}

/// Converts a `tick` to a `price`. Will return `None` for ticks outside MIN_TICK..=MAX_TICK
///
/// This function never panics.
pub fn price_at_tick(tick: Tick) -> Option<Price> {
	if is_tick_valid(tick) {
		Some(sqrt_price_to_price(sqrt_price_at_tick(tick)))
	} else {
		None
	}
}

/// Converts a `price` to a `tick`. Will return `None` is the price is too high or low to be
/// represented by a valid tick i.e. one inside MIN_TICK..=MAX_TICK.
///
/// This function never panics.
pub fn tick_at_price(price: Price) -> Option<Tick> {
	let sqrt_price = price_to_sqrt_price(price);
	if is_sqrt_price_valid(sqrt_price) {
		Some(tick_at_sqrt_price(sqrt_price))
	} else {
		None
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
pub(super) const MIN_SQRT_PRICE: SqrtPriceQ64F96 = U256([0x1000276a3u64, 0x0, 0x0, 0x0]);
/// The maximum value that can be returned from `sqrt_price_at_tick`. Equivalent to
/// `sqrt_price_at_tick(MAX_TICK)`.
pub(super) const MAX_SQRT_PRICE: SqrtPriceQ64F96 =
	U256([0x5d951d5263988d26u64, 0xefd1fc6a50648849u64, 0xfffd8963u64, 0x0u64]);

pub(super) fn is_sqrt_price_valid(sqrt_price: SqrtPriceQ64F96) -> bool {
	(MIN_SQRT_PRICE..=MAX_SQRT_PRICE).contains(&sqrt_price)
}

pub fn is_tick_valid(tick: Tick) -> bool {
	(MIN_TICK..=MAX_TICK).contains(&tick)
}

pub(super) fn sqrt_price_at_tick(tick: Tick) -> SqrtPriceQ64F96 {
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
	(sqrt_price_q32f128 >> 32u128) +
		if sqrt_price_q32f128.low_u32() == 0 { U256::zero() } else { U256::one() }
}

/// Calculates the greatest tick value such that `sqrt_price_at_tick(tick) <= sqrt_price`
pub fn tick_at_sqrt_price(sqrt_price: SqrtPriceQ64F96) -> Tick {
	assert!(is_sqrt_price_valid(sqrt_price));

	let sqrt_price_q64f128 = sqrt_price << 32u128;

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
	} else if sqrt_price_at_tick(tick_high) <= sqrt_price {
		tick_high
	} else {
		tick_low
	}
}

/// Takes a Q128 fixed point number and raises it to the nth power, and returns it as a Q128 fixed
/// point number. If the result is larger than the maximum U384 this function will panic.
///
/// The result will be equal or less than the true value.
pub(super) fn fixed_point_to_power_as_fixed_point(x: U256, n: u32) -> U512 {
	let x = U512::from(x);

	(0..(32 - n.leading_zeros()))
		.zip(
			// This is zipped second and therefore it is not polled if there are no more bits, so
			// we don't calculate x * x one more time than we need, as it may overflow.
			sp_std::iter::once(x).chain(sp_std::iter::repeat_with({
				let mut x = x;
				move || {
					x = (x * x) >> 128;
					x
				}
			})),
		)
		.fold(U512::one() << 128, |total, (i, expo)| {
			if 0x1 << i == (n & 0x1 << i) {
				(total * expo) >> 128
			} else {
				total
			}
		})
}

pub(super) fn nth_root_of_integer_as_fixed_point(x: U256, n: u32) -> U256 {
	// If n is 1 then many x values aren't representable as a fixed point.
	assert!(n > 1);

	let mut root = U256::try_from(
		(0..n.ilog2()).fold(U512::from(x) << 128, |acc, _| (acc << 128).integer_sqrt()),
	)
	.unwrap();

	let x = U512::from(x) << 128;

	for _ in 0..128 {
		let f = fixed_point_to_power_as_fixed_point(root, n);
		let diff = f.abs_diff(x);
		if diff <= f >> 20 {
			break
		} else {
			let delta = mul_div_floor(
				U256::try_from(diff).unwrap(),
				(U256::one() << 128) / U256::from(n),
				fixed_point_to_power_as_fixed_point(root, n - 1),
			);
			root = if f >= x { root - delta } else { root + delta };
		}
	}

	root
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
			assert!(is_sqrt_price_valid(bounded_sqrt_price(
				rng_u256_inclusive_bound(&mut rng, Amount::one()..=Amount::MAX),
				rng_u256_inclusive_bound(&mut rng, Amount::one()..=Amount::MAX),
			)));
		}
	}

	#[cfg(feature = "slow-tests")]
	#[test]
	fn test_increase_sqrt_price() {
		fn inner<SD: SwapDirection>() {
			assert_eq!(SD::increase_sqrt_price(SD::WORST_SQRT_PRICE, 0), SD::WORST_SQRT_PRICE);
			assert_eq!(SD::increase_sqrt_price(SD::WORST_SQRT_PRICE, 1), SD::WORST_SQRT_PRICE);

			let mut rng: rand::rngs::StdRng = rand::rngs::StdRng::from_seed([0; 32]);

			for _i in 0..10000000 {
				let sqrt_price =
					rng_u256_inclusive_bound(&mut rng, (MIN_SQRT_PRICE + 1)..=(MAX_SQRT_PRICE - 1));
				assert!(SD::sqrt_price_op_more_than(
					SD::increase_sqrt_price(sqrt_price, 1),
					sqrt_price
				));
				assert!(SD::sqrt_price_op_more_than(
					SD::increase_sqrt_price(sqrt_price, 10000000),
					sqrt_price
				));
			}

			for tick in MIN_TICK..=MAX_TICK {
				let sqrt_price = sqrt_price_at_tick(tick);
				assert_eq!(sqrt_price, SD::increase_sqrt_price(sqrt_price, 0));
			}
		}

		inner::<BaseToQuote>();
		inner::<QuoteToBase>();
	}

	#[cfg(feature = "slow-tests")]
	#[test]
	fn test_fixed_point_to_power_as_fixed_point() {
		for n in 0..9u32 {
			for e in 0..9u32 {
				assert_eq!(
					U512::from(n.pow(e)) << 128,
					fixed_point_to_power_as_fixed_point(U256::from(n) << 128, e)
				);
			}
		}

		assert_eq!(
			U512::from(57),
			fixed_point_to_power_as_fixed_point(U256::from(3) << 127, 10) >> 128
		);
		assert_eq!(
			U512::from(1) << 128,
			fixed_point_to_power_as_fixed_point(U256::from(2) << 128, 128) >> 128
		);
		assert_eq!(
			U512::from(1) << 255,
			fixed_point_to_power_as_fixed_point(U256::from(2) << 128, 255) >> 128
		);
	}

	#[cfg(feature = "slow-tests")]
	#[test]
	fn test_nth_root_of_integer_as_fixed_point() {
		fn fixed_point_to_float(x: U256) -> f64 {
			x.0.into_iter()
				.fold(0.0f64, |acc, n| (acc / 2.0f64.powi(64)) + (n as f64) * 2.0f64.powi(64))
		}

		for i in 1..100 {
			assert_eq!(
				U256::from(i) << 128,
				nth_root_of_integer_as_fixed_point(U256::from(i * i), 2)
			);
		}

		for n in (0..1000000).step_by(5) {
			for i in 2..100 {
				let root_float = (n as f64).powf(1.0f64 / (i as f64));
				let root = fixed_point_to_float(nth_root_of_integer_as_fixed_point(n.into(), i));

				assert!(
					(root_float - root).abs() <= root_float * 0.000001f64,
					"{root_float} {root}"
				);
			}
		}

		assert_eq!(
			U256::from(2) << 128,
			nth_root_of_integer_as_fixed_point(U256::one() << 128, 128)
		);
		assert_eq!(
			U256::from_dec_str("1198547750512063821665753418683415504682").unwrap(),
			nth_root_of_integer_as_fixed_point(U256::from(83434), 9)
		);
		assert_eq!(
			U256::from_dec_str("70594317847877622574934944024871574448634").unwrap(),
			nth_root_of_integer_as_fixed_point(U256::from(384283294283u128), 5)
		);

		for n in 0..100000u32 {
			let n = U256::from(n);
			for e in 2..10 {
				let root = nth_root_of_integer_as_fixed_point(n, e);
				let x =
					U256::try_from(fixed_point_to_power_as_fixed_point(root, e) >> 128).unwrap();
				assert!((n.saturating_sub(1.into())..=n + 1).contains(&x));
			}
		}
	}

	#[test]
	fn test_mul_div_floor() {
		assert_eq!(mul_div_floor(1.into(), 1.into(), 1), 1.into());
		assert_eq!(mul_div_floor(1.into(), 1.into(), 2), 0.into());
		assert_eq!(mul_div_floor(1.into(), 2.into(), 1), 2.into());
		assert_eq!(mul_div_floor(1.into(), 2.into(), 2), 1.into());
		assert_eq!(mul_div_floor(1.into(), 2.into(), 3), 0.into());
		assert_eq!(mul_div_floor(1.into(), 3.into(), 2), 1.into());
		assert_eq!(mul_div_floor(1.into(), 3.into(), 3), 1.into());
		assert_eq!(mul_div_floor(1.into(), 3.into(), 4), 0.into());
		assert_eq!(mul_div_floor(1.into(), 4.into(), 3), 1.into());
		assert_eq!(mul_div_floor(1.into(), 4.into(), 4), 1.into());
		assert_eq!(mul_div_floor(1.into(), 4.into(), 5), 0.into());
		assert_eq!(mul_div_floor(1.into(), 5.into(), 4), 1.into());
		assert_eq!(mul_div_floor(1.into(), 5.into(), 5), 1.into());
		assert_eq!(mul_div_floor(1.into(), 5.into(), 6), 0.into());

		assert_eq!(mul_div_floor(2.into(), 1.into(), 2), 1.into());
		assert_eq!(mul_div_floor(2.into(), 1.into(), 3), 0.into());
		assert_eq!(mul_div_floor(3.into(), 1.into(), 2), 1.into());
		assert_eq!(mul_div_floor(3.into(), 1.into(), 3), 1.into());
		assert_eq!(mul_div_floor(3.into(), 1.into(), 4), 0.into());
		assert_eq!(mul_div_floor(4.into(), 1.into(), 3), 1.into());
		assert_eq!(mul_div_floor(4.into(), 1.into(), 4), 1.into());
		assert_eq!(mul_div_floor(4.into(), 1.into(), 5), 0.into());
		assert_eq!(mul_div_floor(5.into(), 1.into(), 4), 1.into());
		assert_eq!(mul_div_floor(5.into(), 1.into(), 5), 1.into());
		assert_eq!(mul_div_floor(5.into(), 1.into(), 6), 0.into());

		assert_eq!(mul_div_floor(2.into(), 1.into(), 1), 2.into());
		assert_eq!(mul_div_floor(2.into(), 1.into(), 2), 1.into());

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
			assert_eq!(tick, tick_at_sqrt_price(sqrt_price_at_tick(tick)));
		}
	}

	#[test]
	fn test_sqrt_price_at_tick() {
		assert_eq!(sqrt_price_at_tick(MIN_TICK), MIN_SQRT_PRICE);
		assert_eq!(sqrt_price_at_tick(-738203), U256::from_dec_str("7409801140451").unwrap());
		assert_eq!(sqrt_price_at_tick(-500000), U256::from_dec_str("1101692437043807371").unwrap());
		assert_eq!(
			sqrt_price_at_tick(-250000),
			U256::from_dec_str("295440463448801648376846").unwrap()
		);
		assert_eq!(
			sqrt_price_at_tick(-150000),
			U256::from_dec_str("43836292794701720435367485").unwrap()
		);
		assert_eq!(
			sqrt_price_at_tick(-50000),
			U256::from_dec_str("6504256538020985011912221507").unwrap()
		);
		assert_eq!(
			sqrt_price_at_tick(-5000),
			U256::from_dec_str("61703726247759831737814779831").unwrap()
		);
		assert_eq!(
			sqrt_price_at_tick(-4000),
			U256::from_dec_str("64867181785621769311890333195").unwrap()
		);
		assert_eq!(
			sqrt_price_at_tick(-3000),
			U256::from_dec_str("68192822843687888778582228483").unwrap()
		);
		assert_eq!(
			sqrt_price_at_tick(-2500),
			U256::from_dec_str("69919044979842180277688105136").unwrap()
		);
		assert_eq!(
			sqrt_price_at_tick(-1000),
			U256::from_dec_str("75364347830767020784054125655").unwrap()
		);
		assert_eq!(
			sqrt_price_at_tick(-500),
			U256::from_dec_str("77272108795590369356373805297").unwrap()
		);
		assert_eq!(
			sqrt_price_at_tick(-250),
			U256::from_dec_str("78244023372248365697264290337").unwrap()
		);
		assert_eq!(
			sqrt_price_at_tick(-100),
			U256::from_dec_str("78833030112140176575862854579").unwrap()
		);
		assert_eq!(
			sqrt_price_at_tick(-50),
			U256::from_dec_str("79030349367926598376800521322").unwrap()
		);
		assert_eq!(
			sqrt_price_at_tick(50),
			U256::from_dec_str("79426470787362580746886972461").unwrap()
		);
		assert_eq!(
			sqrt_price_at_tick(100),
			U256::from_dec_str("79625275426524748796330556128").unwrap()
		);
		assert_eq!(
			sqrt_price_at_tick(250),
			U256::from_dec_str("80224679980005306637834519095").unwrap()
		);
		assert_eq!(
			sqrt_price_at_tick(500),
			U256::from_dec_str("81233731461783161732293370115").unwrap()
		);
		assert_eq!(
			sqrt_price_at_tick(1000),
			U256::from_dec_str("83290069058676223003182343270").unwrap()
		);
		assert_eq!(
			sqrt_price_at_tick(2500),
			U256::from_dec_str("89776708723587163891445672585").unwrap()
		);
		assert_eq!(
			sqrt_price_at_tick(3000),
			U256::from_dec_str("92049301871182272007977902845").unwrap()
		);
		assert_eq!(
			sqrt_price_at_tick(4000),
			U256::from_dec_str("96768528593268422080558758223").unwrap()
		);
		assert_eq!(
			sqrt_price_at_tick(5000),
			U256::from_dec_str("101729702841318637793976746270").unwrap()
		);
		assert_eq!(
			sqrt_price_at_tick(50000),
			U256::from_dec_str("965075977353221155028623082916").unwrap()
		);
		assert_eq!(
			sqrt_price_at_tick(150000),
			U256::from_dec_str("143194173941309278083010301478497").unwrap()
		);
		assert_eq!(
			sqrt_price_at_tick(250000),
			U256::from_dec_str("21246587762933397357449903968194344").unwrap()
		);
		assert_eq!(
			sqrt_price_at_tick(500000),
			U256::from_dec_str("5697689776495288729098254600827762987878").unwrap()
		);
		assert_eq!(
			sqrt_price_at_tick(738203),
			U256::from_dec_str("847134979253254120489401328389043031315994541").unwrap()
		);
		assert_eq!(sqrt_price_at_tick(MAX_TICK), MAX_SQRT_PRICE);
	}

	#[test]
	fn test_tick_at_sqrt_price() {
		assert_eq!(tick_at_sqrt_price(MIN_SQRT_PRICE), MIN_TICK);
		assert_eq!(
			tick_at_sqrt_price(U256::from_dec_str("79228162514264337593543").unwrap()),
			-276325
		);
		assert_eq!(
			tick_at_sqrt_price(U256::from_dec_str("79228162514264337593543950").unwrap()),
			-138163
		);
		assert_eq!(
			tick_at_sqrt_price(U256::from_dec_str("9903520314283042199192993792").unwrap()),
			-41591
		);
		assert_eq!(
			tick_at_sqrt_price(U256::from_dec_str("28011385487393069959365969113").unwrap()),
			-20796
		);
		assert_eq!(
			tick_at_sqrt_price(U256::from_dec_str("56022770974786139918731938227").unwrap()),
			-6932
		);
		assert_eq!(
			tick_at_sqrt_price(U256::from_dec_str("79228162514264337593543950336").unwrap()),
			0
		);
		assert_eq!(
			tick_at_sqrt_price(U256::from_dec_str("112045541949572279837463876454").unwrap()),
			6931
		);
		assert_eq!(
			tick_at_sqrt_price(U256::from_dec_str("224091083899144559674927752909").unwrap()),
			20795
		);
		assert_eq!(
			tick_at_sqrt_price(U256::from_dec_str("633825300114114700748351602688").unwrap()),
			41590
		);
		assert_eq!(
			tick_at_sqrt_price(U256::from_dec_str("79228162514264337593543950336000").unwrap()),
			138162
		);
		assert_eq!(
			tick_at_sqrt_price(U256::from_dec_str("79228162514264337593543950336000000").unwrap()),
			276324
		);
		assert_eq!(tick_at_sqrt_price(MAX_SQRT_PRICE - 1), MAX_TICK - 1);
		assert_eq!(tick_at_sqrt_price(MAX_SQRT_PRICE), MAX_TICK);
	}
}
