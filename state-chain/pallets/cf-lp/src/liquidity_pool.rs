use cf_primitives::{liquidity::TradingPosition, Asset, ExchangeRate};
use cf_traits::liquidity::AmmPoolApi;
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use sp_runtime::{
	traits::{One, Zero},
	FixedPointNumber, FixedPointOperand,
};

#[derive(Copy, Clone, Debug, Encode, Decode, MaxEncodedLen, TypeInfo)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct LiquidityPool<Balance> {
	pub enabled: bool,
	asset_0: Asset,
	asset_1: Asset,
	pub liquidity_0: Balance,
	pub liquidity_1: Balance,
}

impl<Balance: Default> LiquidityPool<Balance> {
	pub fn new(asset_0: Asset, asset_1: Asset) -> Self {
		LiquidityPool {
			enabled: false,
			asset_0,
			asset_1,
			liquidity_0: Default::default(),
			liquidity_1: Default::default(),
		}
	}
}

/// Base Amm pool api common to both LPs and swaps.
impl<Balance: FixedPointOperand + Default> AmmPoolApi for LiquidityPool<Balance> {
	type Balance = Balance;
	fn asset_0(&self) -> Asset {
		self.asset_0
	}
	fn asset_1(&self) -> Asset {
		self.asset_1
	}
	fn liquidity_0(&self) -> Self::Balance {
		self.liquidity_0
	}
	fn liquidity_1(&self) -> Self::Balance {
		self.liquidity_1
	}

	fn get_exchange_rate(&self) -> ExchangeRate {
		// TODO: Add exchange rate calculation
		if self.liquidity_1 == Zero::zero() {
			ExchangeRate::one()
		} else {
			ExchangeRate::saturating_from_rational(self.liquidity_0, self.liquidity_1)
		}
	}

	fn get_liquidity_requirement(
		&self,
		position: &TradingPosition<Self::Balance>,
	) -> Option<(Self::Balance, Self::Balance)> {
		// TODO: Add calculation for liquidity requirement
		Some((position.volume_0(), position.volume_0()))
	}
}
