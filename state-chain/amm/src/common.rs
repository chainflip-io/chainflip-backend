use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_core::{U256, U512};

pub const ONE_IN_HUNDREDTH_PIPS: u32 = 1000000;

pub type Amount = U256;
pub type Tick = i32;
pub type SqrtPriceQ64F96 = U256;
pub const SQRT_PRICE_FRACTIONAL_BITS: u32 = 96;

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
)]
pub enum Order {
	Buy,
	Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub enum Side {
	Zero,
	One,
}

impl core::ops::Not for Side {
	type Output = Self;

	fn not(self) -> Self::Output {
		match self {
			Side::Zero => Side::One,
			Side::One => Side::Zero,
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
	Encode,
	Decode,
	MaxEncodedLen,
	Serialize,
	Deserialize,
)]
pub struct SideMap<T> {
	zero: T,
	one: T,
}
impl<T> SideMap<T> {
	pub fn from_array(array: [T; 2]) -> Self {
		let [zero, one] = array;
		Self { zero, one }
	}

	pub fn map<R>(self, mut f: impl FnMut(Side, T) -> R) -> SideMap<R> {
		SideMap { zero: f(Side::Zero, self.zero), one: f(Side::One, self.one) }
	}

	pub fn try_map<R, E>(
		self,
		mut f: impl FnMut(Side, T) -> Result<R, E>,
	) -> Result<SideMap<R>, E> {
		Ok(SideMap { zero: f(Side::Zero, self.zero)?, one: f(Side::One, self.one)? })
	}
}
impl<T> core::ops::Index<Side> for SideMap<T> {
	type Output = T;
	fn index(&self, side: Side) -> &T {
		match side {
			Side::Zero => &self.zero,
			Side::One => &self.one,
		}
	}
}
impl<T> core::ops::IndexMut<Side> for SideMap<T> {
	fn index_mut(&mut self, side: Side) -> &mut T {
		match side {
			Side::Zero => &mut self.zero,
			Side::One => &mut self.one,
		}
	}
}
#[cfg(test)]
impl<T: std::ops::Add<R>, R> std::ops::Add<SideMap<R>> for SideMap<T> {
	type Output = SideMap<<T as std::ops::Add<R>>::Output>;
	fn add(self, rhs: SideMap<R>) -> Self::Output {
		SideMap { zero: self.zero + rhs.zero, one: self.one + rhs.one }
	}
}

pub fn mul_div_floor<C: Into<U512>>(a: U256, b: U256, c: C) -> U256 {
	let c: U512 = c.into();
	(U256::full_mul(a, b) / c).try_into().unwrap()
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

pub struct ZeroToOne {}
pub struct OneToZero {}

pub trait SwapDirection {
	const INPUT_SIDE: Side;

	/// Determines if a given sqrt_price is more than another
	fn sqrt_price_op_more_than(
		sqrt_price: SqrtPriceQ64F96,
		sqrt_price_other: SqrtPriceQ64F96,
	) -> bool;

	/// Returns the equivalent saturated amount in the output asset to a given amount of the input
	/// asset at a specific tick, will return None iff the tick is invalid
	fn input_to_output_amount_floor(amount: Amount, tick: Tick) -> Option<Amount>;
}
impl SwapDirection for ZeroToOne {
	const INPUT_SIDE: Side = Side::Zero;

	fn sqrt_price_op_more_than(
		sqrt_price: SqrtPriceQ64F96,
		sqrt_price_other: SqrtPriceQ64F96,
	) -> bool {
		sqrt_price < sqrt_price_other
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
impl SwapDirection for OneToZero {
	const INPUT_SIDE: Side = Side::One;

	fn sqrt_price_op_more_than(
		sqrt_price: SqrtPriceQ64F96,
		sqrt_price_other: SqrtPriceQ64F96,
	) -> bool {
		sqrt_price > sqrt_price_other
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

pub type Price = U256;
pub const PRICE_FRACTIONAL_BITS: u32 = 128;

pub fn sqrt_price_to_price(sqrt_price: SqrtPriceQ64F96) -> Price {
	assert!((MIN_SQRT_PRICE..=MAX_SQRT_PRICE).contains(&sqrt_price));

	// Note the value here cannot ever be zero as MIN_SQRT_PRICE has its 33th bit set, so sqrt_price
	// will always include a bit pass the 64th bit that is set, so when we shift down below that set
	// bit will not be removed.
	mul_div_floor(
		sqrt_price,
		sqrt_price,
		SqrtPriceQ64F96::one() << (2 * SQRT_PRICE_FRACTIONAL_BITS - PRICE_FRACTIONAL_BITS),
	)
}

pub fn price_to_sqrt_price(price: Price) -> SqrtPriceQ64F96 {
	((U512::from(price) << PRICE_FRACTIONAL_BITS).integer_sqrt() >>
		(PRICE_FRACTIONAL_BITS - SQRT_PRICE_FRACTIONAL_BITS))
		.try_into()
		.unwrap_or(SqrtPriceQ64F96::MAX)
}

pub fn price_at_tick(tick: Tick) -> Option<Price> {
	if is_tick_valid(tick) {
		Some(sqrt_price_to_price(sqrt_price_at_tick(tick)))
	} else {
		None
	}
}

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
pub const MIN_SQRT_PRICE: SqrtPriceQ64F96 = U256([0x1000276a3u64, 0x0, 0x0, 0x0]);
/// The maximum value that can be returned from `sqrt_price_at_tick`. Equivalent to
/// `sqrt_price_at_tick(MAX_TICK)`.
pub const MAX_SQRT_PRICE: SqrtPriceQ64F96 =
	U256([0x5d951d5263988d26u64, 0xefd1fc6a50648849u64, 0xfffd8963u64, 0x0u64]);

pub fn is_sqrt_price_valid(sqrt_price: SqrtPriceQ64F96) -> bool {
	(MIN_SQRT_PRICE..MAX_SQRT_PRICE).contains(&sqrt_price)
}

pub fn is_tick_valid(tick: Tick) -> bool {
	(MIN_TICK..=MAX_TICK).contains(&tick)
}

pub fn sqrt_price_at_tick(tick: Tick) -> SqrtPriceQ64F96 {
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

		Note the increase in I caused by the constant mul will be atleast constant.ilog2().

		Also note each application of `handle_tick_bit` decreases (if the if branch is entered) or else maintains r's value as all the constants are less than 2^128.

		Therefore the largest decrease would be caused if all the macros application's if branches where entered.

		So we assuming all if branches are entered, after all the applications `I` would be atleast I_initial + bigsum(constant.ilog2()) - 19*128.

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
		let mut most_signifcant_bit = 0u8;

		// rustfmt chokes when formatting this macro.
		// See: https://github.com/rust-lang/rustfmt/issues/5404
		#[rustfmt::skip]
		macro_rules! add_integer_bit {
			($bit:literal, $lower_bits_mask:literal) => {
				if _bits_remaining > U256::from($lower_bits_mask) {
					most_signifcant_bit |= $bit;
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
			// most_signifcant_bit is the log2 of sqrt_price_q64f128 as an integer. This
			// converts most_signifcant_bit to the integer log2 of sqrt_price_q64f128 as an
			// q64f128
			((most_signifcant_bit as i16) + (-128i16)) as i8,
			// Calculate mantissa of sqrt_price_q64f128.
			if most_signifcant_bit >= 128u8 {
				// The bits we possibly drop when right shifting don't contribute to the log2
				// above the 14th fractional bit.
				sqrt_price_q64f128 >> (most_signifcant_bit - 127u8)
			} else {
				sqrt_price_q64f128 << (127u8 - most_signifcant_bit)
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

#[cfg(test)]
mod test {
	use super::*;

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
	}
}
