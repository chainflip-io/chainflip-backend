use frame_support::traits::OnRuntimeUpgrade;
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use frame_support::pallet_prelude::DispatchError;
use sp_std::vec::Vec;

use crate::{Config, HistoricalEarnedFees, Pallet};

/// Migrates HistoricalEarnedFees to a DoubleMap.
pub struct Migration<T>(PhantomData<T>);

mod old {
	use super::*;
	use cf_chains::assets;
	use cf_primitives::AssetAmount;
	use codec::{Decode, Encode};
	use frame_support::{pallet_prelude::ValueQuery, Twox64Concat};
	use sp_std::vec;

	#[derive(Encode, Decode, Default, Clone, PartialEq)]
	struct EthMap {
		eth: AssetAmount,
		flip: AssetAmount,
		usdc: AssetAmount,
		usdt: AssetAmount,
	}

	#[derive(Encode, Decode, Default, Clone, PartialEq)]
	struct DotMap {
		dot: AssetAmount,
	}

	#[derive(Encode, Decode, Default, Clone, PartialEq)]
	struct BtcMap {
		btc: AssetAmount,
	}

	#[derive(Encode, Decode, Default, Clone, PartialEq)]
	pub struct AssetMap {
		eth: EthMap,
		dot: DotMap,
		btc: BtcMap,
	}

	impl AssetMap {
		pub fn iter(&self) -> impl Iterator<Item = (assets::any::Asset, AssetAmount)> {
			vec![
				(assets::any::Asset::Eth, self.eth.eth),
				(assets::any::Asset::Flip, self.eth.flip),
				(assets::any::Asset::Usdc, self.eth.usdc),
				(assets::any::Asset::Usdt, self.eth.usdt),
				(assets::any::Asset::Dot, self.dot.dot),
				(assets::any::Asset::Btc, self.btc.btc),
			]
			.into_iter()
		}
	}

	#[frame_support::storage_alias]
	pub type HistoricalEarnedFees<T: Config> = StorageMap<
		Pallet<T>,
		Twox64Concat,
		<T as frame_system::Config>::AccountId,
		AssetMap,
		ValueQuery,
	>;
}

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		for (account_id, asset_map) in old::HistoricalEarnedFees::<T>::drain().collect::<Vec<_>>() {
			for (asset, amount) in asset_map.iter() {
				if amount != 0 {
					HistoricalEarnedFees::<T>::insert(&account_id, asset, amount);
				}
			}
		}
		Default::default()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		use cf_primitives::chains::assets::any::AssetMap as NewAssetMap;
		use codec::Encode;
		use sp_std::collections::btree_map::BTreeMap;

		let old_asset_maps: BTreeMap<T::AccountId, NewAssetMap<_>> =
			old::HistoricalEarnedFees::<T>::iter()
				.map(|(account_id, old_asset_map)| {
					(account_id, NewAssetMap::from_iter(old_asset_map.iter()))
				})
				.collect();
		Ok(old_asset_maps.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		use cf_primitives::chains::assets::any::AssetMap as NewAssetMap;
		use codec::Decode;
		use frame_support::ensure;
		use sp_std::collections::btree_map::BTreeMap;

		let old_asset_maps: BTreeMap<T::AccountId, NewAssetMap<u128>> =
			Decode::decode(&mut &state[..])
				.or(Err(DispatchError::Other("Failed to decode state")))?;
		for (account_id, asset_map) in old_asset_maps {
			for (asset, amount) in asset_map.iter() {
				ensure!(
					HistoricalEarnedFees::<T>::get(&account_id, asset) == *amount,
					"Post-upgrade amounts do not match for account {:?} and asset {:?}",
				)
			}
		}
		Ok(())
	}
}
