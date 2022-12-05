#![cfg_attr(not(feature = "std"), no_std)]
use cf_primitives::{chains::assets::any, AssetAmount, ExchangeRate, PoolAsset, TradingPosition};
use cf_traits::Chainflip;
use frame_support::{pallet_prelude::*, sp_runtime::traits::Saturating};

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub(crate) mod mini_pool {
	use super::*;

	#[derive(
		Copy, Clone, Debug, Default, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo,
	)]
	pub struct AmmPool {
		asset_0: AssetAmount,
		asset_1: AssetAmount,
	}

	impl AmmPool {
		pub fn add_liquidity(&mut self, volume_0: AssetAmount, volume_1: AssetAmount) {
			self.asset_0.saturating_accrue(volume_0);
			self.asset_1.saturating_accrue(volume_1);
		}

		pub fn swap_rate(&self, input_amount: AssetAmount) -> AssetAmount {
			self.asset_1 / (self.asset_0 + input_amount)
		}

		pub fn swap(&mut self, input_amount: AssetAmount) -> AssetAmount {
			let output_amount = self.swap_rate(input_amount) * input_amount;
			self.asset_0.saturating_accrue(input_amount);
			self.asset_1.saturating_reduce(output_amount);
			output_amount
		}

		pub fn reverse_swap(&mut self, input_amount: AssetAmount) -> AssetAmount {
			self.in_reverse(|reversed| reversed.swap(input_amount))
		}

		fn reversed(self) -> Self {
			Self { asset_0: self.asset_1, asset_1: self.asset_0 }
		}

		fn in_reverse<R, F: FnOnce(&mut Self) -> R>(&mut self, f: F) -> R {
			let mut reversed = self.reversed();
			let r = f(&mut reversed);
			*self = reversed.reversed();
			r
		}
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {}

	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(PhantomData<T>);

	/// Pools are indexed by single asset since USDC is implicit.
	#[pallet::storage]
	pub(super) type Pools<T: Config> =
		StorageMap<_, Twox64Concat, any::Asset, mini_pool::AmmPool, ValueQuery>;
}

impl<T: Config> cf_traits::SwappingApi for Pallet<T> {
	fn swap(
		from: any::Asset,
		to: any::Asset,
		input_amount: AssetAmount,
		_fee: u16,
	) -> (AssetAmount, (cf_primitives::Asset, AssetAmount)) {
		(
			match (from, to) {
				(input_asset, any::Asset::Usdc) =>
					Pools::<T>::mutate(input_asset, |pool| pool.swap(input_amount)),
				(any::Asset::Usdc, output_asset) =>
					Pools::<T>::mutate(output_asset, |pool| pool.reverse_swap(input_amount)),
				(input_asset, output_asset) => Pools::<T>::mutate(output_asset, |pool| {
					pool.reverse_swap(Pools::<T>::mutate(input_asset, |pool| {
						pool.swap(input_amount)
					}))
				}),
			},
			(any::Asset::Usdc, 0),
		)
	}
}

impl<T: Config> cf_traits::LiquidityPoolApi for Pallet<T> {
	fn deploy(asset: &any::Asset, position: cf_primitives::TradingPosition<AssetAmount>) {
		match position {
			TradingPosition::ClassicV3 { volume_0, volume_1, .. } => {
				Pools::<T>::mutate(asset, |pool| pool.add_liquidity(volume_0, volume_1));
			},
			TradingPosition::VolatileV3 { side, volume, .. } => {
				Pools::<T>::mutate(asset, |pool| match side {
					PoolAsset::Asset0 => pool.add_liquidity(volume, 0),
					PoolAsset::Asset1 => pool.add_liquidity(0, volume),
				});
			},
		}
	}

	fn add_liquidity(
		_asset: &any::Asset,
		_amount: AssetAmount,
		_stable_amount: AssetAmount,
	) -> DispatchResult {
		Ok(())
	}

	fn remove_liquidity(
		_asset: &any::Asset,
		_amount: AssetAmount,
		_stable_amount: AssetAmount,
	) -> DispatchResult {
		Ok(())
	}

	fn get_liquidity(_asset: &any::Asset) -> (AssetAmount, AssetAmount) {
		(0, 0)
	}

	fn get_exchange_rate(_asset: &any::Asset) -> ExchangeRate {
		Default::default()
	}

	fn get_liquidity_requirement(
		_asset: &any::Asset,
		_position: &TradingPosition<AssetAmount>,
	) -> Option<(AssetAmount, AssetAmount)> {
		None
	}

	fn get_stable_asset() -> any::Asset {
		any::Asset::Usdc
	}
}
