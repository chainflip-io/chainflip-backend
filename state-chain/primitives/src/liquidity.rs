use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

use crate::chains::assets::any::Asset;

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

pub type PoolId = (Asset, Asset);

/// The id type for range orders.
pub type PositionId = u64;

/// The type used for measuring price ticks. Note Uniswap uses i24 but this is not supported in
/// rust.
pub type Tick = i32;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct AmmRange {
	pub lower: Tick,
	pub upper: Tick,
}

/// Denotes the two assets contained in a pool.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum PoolAsset {
	Asset0,
	Asset1,
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
