use crate::{Asset, AssetAmount};
use frame_support::sp_runtime::traits::Saturating;

#[must_use = "AssetBalance must be burned before dropping"]
#[derive(Debug, PartialEq, Eq)]
pub struct AssetBalance {
	amount: AssetAmount,
	asset: Asset,
}

impl AssetBalance {
	pub fn mint(amount: AssetAmount, asset: Asset) -> Self {
		Self { amount, asset }
	}

	pub fn amount(&self) -> AssetAmount {
		self.amount
	}

	pub fn burn(mut self) -> AssetAmount {
		core::mem::take(&mut self.amount)
	}

	pub fn accrue(&mut self, other: Self) {
		debug_assert_eq!(self.asset, other.asset, "AssetBalance::deposit: asset mismatch");
		self.amount.saturating_accrue(other.burn());
	}

	pub fn take(&mut self, amount: AssetAmount) -> Option<Self> {
		if self.amount < amount {
			return None;
		}
		self.amount -= amount;
		Some(Self { amount, asset: self.asset })
	}

	pub fn take_saturating(&mut self, amount: AssetAmount) -> Self {
		let taken = self.amount.min(amount);
		self.amount -= taken;
		Self { amount: taken, asset: self.asset }
	}
}

impl Drop for AssetBalance {
	fn drop(&mut self) {
		debug_assert!(self.amount == 0, "AssetBalance was not burned before dropping");
	}
}

#[cfg(feature = "std")]
impl core::fmt::Display for AssetBalance {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let decimals = match self.asset {
			Asset::Dot => 10,
			Asset::Eth => 18,
			Asset::Flip => 18,
			Asset::Usdc => 18,
			Asset::Usdt => 18,
			Asset::Btc => 8,
			Asset::ArbEth => 18,
			Asset::ArbUsdc => 18,
			Asset::Sol => todo!(),
		};
		let amount = self.amount as f64 / 10f64.powi(decimals);
		write!(f, "{} {}", amount, self.asset)
	}
}

#[cfg(test)]
mod tests {
	use cf_utilities::assert_panics;

	use super::*;

	#[test]
	fn test_asset_balance() {
		let mut balance = AssetBalance::mint(100, Asset::Dot);
		assert_eq!(balance.amount(), 100);

		let other = AssetBalance::mint(50, Asset::Dot);
		balance.accrue(other);
		assert_eq!(balance.amount(), 150);

		let taken = balance.take(100).unwrap();
		assert_eq!(taken.amount(), 100);
		assert_eq!(balance.amount(), 50);
		assert_eq!(taken.burn(), 100);

		let taken = balance.take_saturating(100);
		assert_eq!(taken.amount(), 50);
		assert_eq!(balance.amount(), 0);
		assert_eq!(taken.burn(), 50);

		#[cfg(debug_assertions)]
		{
			assert_panics!({
				let _ = AssetBalance::mint(100, Asset::Dot);
			});
			assert_panics!({
				let mut balance = AssetBalance::mint(100, Asset::Dot);
				balance.accrue(AssetBalance::mint(50, Asset::Eth));
				balance.burn();
			});
		}
	}
}
