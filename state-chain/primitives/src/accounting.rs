use crate::{Asset, AssetAmount};
use cf_runtime_utilities::log_or_panic;
use codec::{Decode, Encode};
use frame_support::sp_runtime::traits::Saturating;
use scale_info::TypeInfo;

/// Represents an asset balance. This is a simple struct that holds an asset and the amount of that
/// asset. It provides methods to add, subtract, and compare balances in a more secure way that
/// gives more guarantees about compatibility and resource handling. We want to use this as a
/// replacement for the AssetAmount type where it is possible and straight forward. It's intended
/// that this type is not deriving Copy or Clone to force the user to think about the handling of
/// the resource.
#[derive(Debug, Encode, Decode, TypeInfo, Eq)]
pub struct AssetBalance {
	/// The asset of the balance. For example, DOT, ETH, etc.
	asset: Asset,
	/// The amount of the balance.
	amount: AssetAmount,
}

impl AssetBalance {
	/// Creates a new balance with the given amount and asset.
	pub fn new(asset: Asset, amount: AssetAmount) -> Self {
		Self { asset, amount }
	}

	/// Checks if the balance is zero.
	pub fn is_zero(&self) -> bool {
		self.amount == 0
	}

	/// Returns the amount of the asset.
	pub fn amount(&self) -> AssetAmount {
		self.amount
	}

	/// Returns the asset.
	pub fn asset(&self) -> Asset {
		self.asset
	}

	/// Ensures we consume the other asset, checks the compatibility and adds it to asset balance
	/// saturating at MAX.
	pub fn saturating_accrue(&mut self, other: Self) {
		Self::ensure_asset_compatibility(self, &other);
		self.amount.saturating_accrue(other.amount());
	}

	/// Ensures we consume the other asset, checks the compatibility and reduces it from asset
	/// balance saturating at 0.
	pub fn saturating_reduce(&mut self, other: Self) {
		Self::ensure_asset_compatibility(self, &other);
		self.amount.saturating_reduce(other.amount());
	}

	/// Ensures we consume the other asset, checks the compatibility and adds it to asset balance.
	/// Wraps the actual checked_add method and so provides the same functionality. Doesn't modify
	/// the original balance.
	pub fn checked_add(&self, other: Self) -> Option<Self> {
		Self::ensure_asset_compatibility(self, &other);
		self.amount
			.checked_add(other.amount)
			.map(|result| Self { amount: result, asset: self.asset })
	}

	/// Ensures we consume the other asset, checks the compatibility and subtracts it from asset
	/// balance. Wraps the actual checked_sub method and so provides the same functionality. Doesn't
	/// modify the original balance.
	pub fn checked_sub(&self, other: Self) -> Option<Self> {
		Self::ensure_asset_compatibility(self, &other);
		self.amount
			.checked_sub(other.amount)
			.map(|result| Self { amount: result, asset: self.asset })
	}

	/// Subtracts the given amount from the balance, saturating at 0.
	/// Note: This is a primitive operation and should be used with caution.
	/// It is the caller's responsibility to ensure **not** to mix assets.
	pub fn saturating_sub_amount(&mut self, amount: AssetAmount) {
		self.amount = self.amount.saturating_sub(amount);
	}

	/// Adds the given amount to the balance, saturating at MAX.
	/// Note: This is a primitive operation and should be used with caution.
	/// It is the caller's responsibility to ensure **not** to mix assets.
	pub fn saturating_add_amount(&mut self, amount: AssetAmount) {
		self.amount = self.amount.saturating_add(amount);
	}

	/// Ensures that the asset of the two balances are the same.
	fn ensure_asset_compatibility(&self, other: &Self) {
		if self.asset != other.asset {
			log_or_panic!("Mixing assets: {:?} and {:?}!", self.asset, other.asset);
		}
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
			Asset::Sol => 9,
			Asset::SolUsdc => 6,
		};
		let amount = self.amount as f64 / 10f64.powi(decimals);
		write!(f, "{} {}", amount, self.asset)
	}
}

#[cfg(test)]
mod tests {

	use super::*;

	#[cfg(test)]
	mod ensure_to_not_mix_assets {

		use super::*;

		// Implements a test that takes a body and expects it to panic.
		#[macro_export]
		macro_rules! panic_when_mix_assets {
			($test_name:ident, $body:expr) => {
				#[test]
				#[should_panic]
				fn $test_name() {
					$body
				}
			};
		}

		panic_when_mix_assets!(add, {
			AssetBalance::new(Asset::Dot, 100).saturating_accrue(AssetBalance::new(Asset::Eth, 50));
		});

		panic_when_mix_assets!(sub, {
			AssetBalance::new(Asset::Dot, 100).saturating_reduce(AssetBalance::new(Asset::Eth, 50));
		});

		panic_when_mix_assets!(checked_add, {
			AssetBalance::new(Asset::Dot, 100).checked_add(AssetBalance::new(Asset::Eth, 50));
		});

		panic_when_mix_assets!(checked_sub, {
			AssetBalance::new(Asset::Dot, 100).checked_sub(AssetBalance::new(Asset::Eth, 50));
		});

		panic_when_mix_assets!(lt, {
			let _ = AssetBalance::new(Asset::Dot, 100) < AssetBalance::new(Asset::Eth, 50);
		});

		panic_when_mix_assets!(le, {
			let _ = AssetBalance::new(Asset::Dot, 100) <= AssetBalance::new(Asset::Eth, 50);
		});

		panic_when_mix_assets!(gt, {
			let _ = AssetBalance::new(Asset::Dot, 100) > AssetBalance::new(Asset::Eth, 50);
		});

		panic_when_mix_assets!(ge, {
			let _ = AssetBalance::new(Asset::Dot, 100) >= AssetBalance::new(Asset::Eth, 50);
		});

		panic_when_mix_assets!(eq, {
			let _ = AssetBalance::new(Asset::Dot, 100) == AssetBalance::new(Asset::Eth, 50);
		});

		panic_when_mix_assets!(ne, {
			let _ = AssetBalance::new(Asset::Dot, 100) != AssetBalance::new(Asset::Eth, 50);
		});

		panic_when_mix_assets!(partial_cmp, {
			let _ =
				AssetBalance::new(Asset::Dot, 100).partial_cmp(&AssetBalance::new(Asset::Eth, 50));
		});
	}

	// Proofs we can add asset A with B and consume B.
	#[test]
	fn add_and_consume_balance() {
		let mut balance = AssetBalance::new(Asset::Dot, 100);
		let other = AssetBalance::new(Asset::Dot, 50);

		balance.saturating_accrue(other);
		assert_eq!(balance.amount(), 150);
	}

	// Proofs we can subtract asset A with B and consume B.
	#[test]
	fn sub_and_consume_balance() {
		let mut balance = AssetBalance::new(Asset::Dot, 100);
		let other = AssetBalance::new(Asset::Dot, 50);

		balance.saturating_reduce(other);
		assert_eq!(balance.amount(), 50);
	}

	// Proofs that we can manipulate a raw amount of the balance.
	#[test]
	fn scalar_operations() {
		let mut balance = AssetBalance::new(Asset::Dot, 100);

		balance.saturating_add_amount(50);
		assert_eq!(balance.amount(), 150);
		balance.saturating_sub_amount(50);
		assert_eq!(balance.amount(), 100);
	}

	/// Proofs that the overloaded operators work as expected.
	#[test]
	fn overloaded_operators() {
		let balance_a = AssetBalance::new(Asset::Dot, 100);
		let balance_b = AssetBalance::new(Asset::Dot, 50);

		assert!(balance_a > balance_b);
		assert!(balance_a >= balance_b);
		assert!(balance_b < balance_a);
		assert!(balance_b <= balance_a);
		assert!(balance_a != balance_b);
		assert!(balance_a == balance_a);
	}
}
