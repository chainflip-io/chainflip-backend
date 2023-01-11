use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::U256;
use sp_std::ops::{Index, IndexMut, Not};

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

/// Representation of a price: p = 1.0001^Tick
pub type Tick = i32;

/// Representation of Liquidity in an exchange pool.
pub type Liquidity = u128;

/// Amount used to calculate exchange pool
pub type AmountU256 = U256;

/// sqrt(Price) in amm exchange Pool. Q64.96 numerical type.
pub type SqrtPriceQ64F96 = U256;

/// Q128.128 numerical type use to record Fee.
pub type FeeGrowthQ128F128 = U256;

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
pub enum PoolAsset {
	Asset0,
	Asset1,
}

impl Not for PoolAsset {
	type Output = Self;

	fn not(self) -> Self::Output {
		match self {
			PoolAsset::Asset0 => PoolAsset::Asset1,
			PoolAsset::Asset1 => PoolAsset::Asset0,
		}
	}
}

/// Represents the types of order that an LP can submit to the AMM.
#[derive(Copy, Clone, Debug, TypeInfo, PartialEq, Eq, Encode, Decode, MaxEncodedLen)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum TradingPosition<Volume> {
	/// Standard Uniswap V3 style 'sticky' range order position. When executed, the converted
	/// amount remains in the pool.
	///
	/// The volumes must be consistent with the specified range.
	ClassicV3 { range: AmmRange, volume_0: Volume, volume_1: Volume },
	/// A 'volatile' single-sided position. When executed, the converted amount is returned to the
	/// LP's free balance.
	VolatileV3 { range: AmmRange, side: PoolAsset, volume: Volume },
}

impl<Volume: Default + Copy> TradingPosition<Volume> {
	pub fn volume_0(&self) -> Volume {
		match self {
			TradingPosition::ClassicV3 { volume_0, .. } => *volume_0,
			TradingPosition::VolatileV3 { side, volume, .. } if *side == PoolAsset::Asset0 =>
				*volume,
			_ => Default::default(),
		}
	}
	pub fn volume_1(&self) -> Volume {
		match self {
			TradingPosition::ClassicV3 { volume_1, .. } => *volume_1,
			TradingPosition::VolatileV3 { side, volume, .. } if *side == PoolAsset::Asset1 =>
				*volume,
			_ => Default::default(),
		}
	}
}

/// A costom type that contains two `Amount`s, one for each side of an exchange pool.
#[derive(Copy, Clone, Debug, TypeInfo, PartialEq, Eq, Encode, Decode, MaxEncodedLen)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct PoolAssetMap<Amount> {
	asset_0: Amount,
	asset_1: Amount,
}

impl<Amount: Clone> PoolAssetMap<Amount> {
	pub fn new(asset_0: Amount, asset_1: Amount) -> Self {
		Self { asset_0, asset_1 }
	}

	/// Creates a new PooAssetMap from
	pub fn new_from_fn(f: impl Fn(PoolAsset) -> Amount) -> Self {
		Self { asset_0: f(PoolAsset::Asset0), asset_1: f(PoolAsset::Asset1) }
	}

	pub fn mutate(&mut self, f: impl Fn(PoolAsset, Amount) -> Amount) {
		self.asset_0 = f(PoolAsset::Asset0, self.asset_0.clone());
		self.asset_1 = f(PoolAsset::Asset1, self.asset_1.clone());
	}
}

impl<Amount> Index<PoolAsset> for PoolAssetMap<Amount> {
	type Output = Amount;
	fn index(&self, side: PoolAsset) -> &Amount {
		match side {
			PoolAsset::Asset0 => &self.asset_0,
			PoolAsset::Asset1 => &self.asset_1,
		}
	}
}

impl<Amount> IndexMut<PoolAsset> for PoolAssetMap<Amount> {
	fn index_mut(&mut self, side: PoolAsset) -> &mut Amount {
		match side {
			PoolAsset::Asset0 => &mut self.asset_0,
			PoolAsset::Asset1 => &mut self.asset_1,
		}
	}
}

impl<Amount: Default> Default for PoolAssetMap<Amount> {
	fn default() -> Self {
		Self { asset_0: Default::default(), asset_1: Default::default() }
	}
}

impl From<PoolAssetMap<u128>> for PoolAssetMap<U256> {
	fn from(asset_map: PoolAssetMap<u128>) -> Self {
		Self::new(asset_map[PoolAsset::Asset0].into(), asset_map[PoolAsset::Asset1].into())
	}
}

impl From<PoolAssetMap<U256>> for PoolAssetMap<u128> {
	fn from(asset_map: PoolAssetMap<U256>) -> Self {
		Self::new(asset_map[PoolAsset::Asset0].as_u128(), asset_map[PoolAsset::Asset1].as_u128())
	}
}

// Simple struct used to represent an minted Liquidity position.
#[derive(Copy, Clone, Default, Eq, PartialEq, Debug)]
pub struct MintedLiquidity {
	pub range: AmmRange,
	pub liquidity: Liquidity,
	pub fees_acrued: PoolAssetMap<u128>,
}
