use cf_traits::liquidity::{AmmPoolApi, Asset, ExchangeRate, TradingPosition};
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_runtime::{
	traits::{One, Zero},
	FixedPointNumber, FixedPointOperand,
};

#[derive(Copy, Clone, Debug, Encode, Decode, MaxEncodedLen, Serialize, Deserialize, TypeInfo)]
pub struct LiquidityPool<Balance> {
	pub asset0: Asset,
	pub asset1: Asset,
	pub liquidity0: Balance,
	pub liquidity1: Balance,
}

impl<Balance: Default> LiquidityPool<Balance> {
	pub fn new(asset0: Asset, asset1: Asset) -> Self {
		LiquidityPool {
			asset0,
			asset1,
			liquidity0: Default::default(),
			liquidity1: Default::default(),
		}
	}
}

/// Base Amm pool api common to both LPs and swaps.
impl<Balance: FixedPointOperand + Default> AmmPoolApi for LiquidityPool<Balance> {
	type Balance = Balance;
	fn asset_0(&self) -> Asset {
		self.asset0
	}
	fn asset_1(&self) -> Asset {
		self.asset1
	}
	fn liquidity_0(&self) -> Self::Balance {
		self.liquidity0
	}
	fn liquidity_1(&self) -> Self::Balance {
		self.liquidity1
	}

	fn get_exchange_rate(&self) -> ExchangeRate {
		// TODO: Add exchange rate calculation
		if self.liquidity1 == Zero::zero() {
			ExchangeRate::one()
		} else {
			ExchangeRate::saturating_from_rational(self.liquidity0, self.liquidity1)
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
