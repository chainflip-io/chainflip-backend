use crate::{Asset, AssetAmount};
use codec::{Decode, Encode};
use frame_support::sp_runtime::traits::Saturating;
use scale_info::TypeInfo;
use sp_std::ops::{Add, Sub};

// Note: Do not implement Clone for AssetBalance. It is not safe to clone AssetBalance!
#[must_use = "AssetBalance must be burned before dropping"]
#[derive(Debug, Encode, Decode, TypeInfo, Eq)]
pub struct AssetBalance {
	amount: AssetAmount,
	asset: Asset,
}

impl AssetBalance {
	pub fn is_zero(&self) -> bool {
		self.amount == 0
	}

	pub fn mint(amount: AssetAmount, asset: Asset) -> Self {
		Self { amount, asset }
	}

	pub fn amount(&self) -> AssetAmount {
		self.amount
	}

	pub fn asset(&self) -> Asset {
		self.asset
	}

	/// Consumes the other asset, burns it and adds it to the balance.
	pub fn accrue(&mut self, other: Self) {
		Self::ensure_asset_compatibility(self, &other);
		self.amount.saturating_accrue(other.amount());
	}

	pub fn checked_add(&self, other: Self) -> Option<Self> {
		Self::ensure_asset_compatibility(self, &other);
		self.amount
			.checked_add(other.amount)
			.map(|result| Self { amount: result, asset: self.asset })
	}

	pub fn checked_sub(&self, other: Self) -> Option<Self> {
		Self::ensure_asset_compatibility(self, &other);
		self.amount
			.checked_sub(other.amount)
			.map(|result| Self { amount: result, asset: self.asset })
	}

	pub fn reduce(&mut self, other: Self) {
		Self::ensure_asset_compatibility(self, &other);
		self.amount.saturating_reduce(other.amount());
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

	/// Subtracts the given amount from the balance, saturating at 0.
	/// Note: This is a primitive operation and should be used with caution.
	/// It is the caller's responsibility to ensure **not** to mix assets.
	pub fn saturating_primitive_sub(&mut self, amount: AssetAmount) {
		self.amount = self.amount.saturating_sub(amount);
	}

	/// Adds the given amount to the balance, saturating at MAX.
	/// Note: This is a primitive operation and should be used with caution.
	/// It is the caller's responsibility to ensure **not** to mix assets.
	pub fn saturating_primitive_add(&mut self, amount: AssetAmount) {
		self.amount = self.amount.saturating_add(amount);
	}

	/// Ensures that the asset of the two balances is the same.
	fn ensure_asset_compatibility(&self, other: &Self) {
		debug_assert_eq!(self.asset, other.asset, "AssetBalance: asset mismatch");
	}
}

impl Ord for AssetBalance {
	fn cmp(&self, other: &Self) -> core::cmp::Ordering {
		Self::ensure_asset_compatibility(self, other);
		self.amount.cmp(&other.amount)
	}
}

impl PartialEq for AssetBalance {
	fn eq(&self, other: &Self) -> bool {
		Self::ensure_asset_compatibility(self, other);
		self.amount == other.amount
	}

	#[allow(clippy::partialeq_ne_impl)]
	fn ne(&self, other: &Self) -> bool {
		Self::ensure_asset_compatibility(self, other);
		!self.eq(other)
	}
}

impl PartialOrd for AssetBalance {
	#[allow(clippy::non_canonical_partial_ord_impl)]
	fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
		Self::ensure_asset_compatibility(self, other);
		Some(self.cmp(other))
	}

	fn lt(&self, other: &Self) -> bool {
		Self::ensure_asset_compatibility(self, other);
		self.amount < other.amount
	}

	fn le(&self, other: &Self) -> bool {
		Self::ensure_asset_compatibility(self, other);
		self.amount <= other.amount
	}

	fn gt(&self, other: &Self) -> bool {
		Self::ensure_asset_compatibility(self, other);
		self.amount > other.amount
	}

	fn ge(&self, other: &Self) -> bool {
		Self::ensure_asset_compatibility(self, other);
		self.amount >= other.amount
	}
}

impl Add for AssetBalance {
	type Output = Self;

	fn add(self, other: Self) -> Self {
		Self::ensure_asset_compatibility(&self, &other);
		Self { amount: self.amount + other.amount, asset: self.asset }
	}
}

impl Sub for AssetBalance {
	type Output = Self;

	fn sub(self, other: Self) -> Self {
		Self::ensure_asset_compatibility(&self, &other);
		Self { amount: self.amount - other.amount, asset: self.asset }
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
			Asset::SolUsdc => todo!(),
		};
		let amount = self.amount as f64 / 10f64.powi(decimals);
		write!(f, "{} {}", amount, self.asset)
	}
}

#[cfg(test)]
mod tests {

	use super::*;

	// #[test]
	// fn test_asset_balance() {
	// 	let mut balance = AssetBalance::mint(100, Asset::Dot);
	// 	assert_eq!(balance.amount(), 100);

	// 	let other = AssetBalance::mint(50, Asset::Dot);
	// 	balance.accrue(other);
	// 	assert_eq!(balance.amount(), 150);

	// 	let taken = balance.take(100).unwrap();
	// 	assert_eq!(taken.amount(), 100);
	// 	assert_eq!(balance.amount(), 50);
	// 	assert_eq!(taken.burn(), 100);

	// 	let taken = balance.take_saturating(100);
	// 	assert_eq!(taken.amount(), 50);
	// 	assert_eq!(balance.amount(), 0);
	// 	assert_eq!(taken.burn(), 50);
	// }

	#[test]
	fn test_accure() {
		let mut balance = AssetBalance::mint(100, Asset::Dot);
		let other = AssetBalance::mint(50, Asset::Dot);
		balance.accrue(other);
		assert_eq!(balance.amount(), 150);
	}
}
