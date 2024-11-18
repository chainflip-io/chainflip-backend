use cf_amm_math::*;
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_core::{U256, U512};

pub const ONE_IN_HUNDREDTH_PIPS: u32 = 1_000_000;
pub const MAX_LP_FEE: u32 = ONE_IN_HUNDREDTH_PIPS / 2;

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
	PartialOrd,
	Ord,
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
	use cf_amm_math::test_utilities::rng_u256_inclusive_bound;

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
}
