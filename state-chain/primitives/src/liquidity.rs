use crate::AssetAmount;
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
pub use sp_core::U256;
use sp_std::ops::{Index, IndexMut, Not};

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

/// Representation of a price: p = 1.0001^Tick
pub type Tick = i32;

/// Representation of Liquidity in an exchange pool.
pub type Liquidity = u128;

/// Amount used to calculate exchange pool
pub type AmountU256 = U256;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct AmmRange {
	pub lower: Tick,
	pub upper: Tick,
}

impl AmmRange {
	pub fn new(lower: Tick, upper: Tick) -> Self {
		Self { lower, upper }
	}
}

/// Denotes the two assets contained in a pool.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum PoolSide {
	Asset0,
	Asset1,
}

impl Not for PoolSide {
	type Output = Self;

	fn not(self) -> Self::Output {
		match self {
			PoolSide::Asset0 => PoolSide::Asset1,
			PoolSide::Asset1 => PoolSide::Asset0,
		}
	}
}

/// A custom type that contains two `Amount`s, one for each side of an exchange pool.
#[derive(Copy, Clone, Default, Debug, TypeInfo, PartialEq, Eq, Encode, Decode, MaxEncodedLen)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct PoolAssetMap<Amount> {
	asset_0: Amount,
	asset_1: Amount,
}

impl<Amount: Copy> PoolAssetMap<Amount> {
	pub fn new(asset_0: Amount, asset_1: Amount) -> Self {
		Self { asset_0, asset_1 }
	}

	/// Creates a new PooAssetMap from a function f
	pub fn new_from_fn(f: impl Fn(PoolSide) -> Amount) -> Self {
		Self { asset_0: f(PoolSide::Asset0), asset_1: f(PoolSide::Asset1) }
	}

	pub fn mutate(&mut self, mut f: impl FnMut(PoolSide, &mut Amount)) {
		f(PoolSide::Asset0, &mut self.asset_0);
		f(PoolSide::Asset1, &mut self.asset_1);
	}
}

impl<Amount> Index<PoolSide> for PoolAssetMap<Amount> {
	type Output = Amount;
	fn index(&self, side: PoolSide) -> &Amount {
		match side {
			PoolSide::Asset0 => &self.asset_0,
			PoolSide::Asset1 => &self.asset_1,
		}
	}
}

impl<Amount> IndexMut<PoolSide> for PoolAssetMap<Amount> {
	fn index_mut(&mut self, side: PoolSide) -> &mut Amount {
		match side {
			PoolSide::Asset0 => &mut self.asset_0,
			PoolSide::Asset1 => &mut self.asset_1,
		}
	}
}

impl From<PoolAssetMap<u128>> for PoolAssetMap<U256> {
	fn from(asset_map: PoolAssetMap<u128>) -> Self {
		Self::new(asset_map[PoolSide::Asset0].into(), asset_map[PoolSide::Asset1].into())
	}
}

/// Attempts to cast U256s to u128s, failing on overflow.
impl TryFrom<PoolAssetMap<U256>> for PoolAssetMap<u128> {
	type Error = <u128 as TryFrom<U256>>::Error;

	fn try_from(value: PoolAssetMap<U256>) -> Result<Self, Self::Error> {
		Ok(Self::new(value.asset_0.try_into()?, value.asset_1.try_into()?))
	}
}

#[derive(Copy, Clone, Default, Eq, PartialEq, Debug)]
pub struct BurnResult {
	pub assets_returned: PoolAssetMap<AssetAmount>,
	pub fees_accrued: PoolAssetMap<AssetAmount>,
}

impl BurnResult {
	pub fn new(
		assets_returned: PoolAssetMap<AssetAmount>,
		fees_accrued: PoolAssetMap<AssetAmount>,
	) -> Self {
		Self { assets_returned, fees_accrued }
	}
}
