use crate::{Config, Pallet};
use frame_support::traits::{OnRuntimeUpgrade, PalletInfoAccess};

#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

fn storage_names() -> impl Iterator<Item = &'static [u8]> {
	[&b"FlipBuyInterval"[..], &b"CollectedNetworkFee"[..]].into_iter()
}
pub struct Migration<T: Config>(sp_std::marker::PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		// Moving the FlipBuyInterval & CollectedNetworkFee storage items from the pools
		// pallet to the swapping pallet.
		cf_runtime_upgrade_utilities::move_pallet_storage_to::<Pallet<T>>(
			b"FlipBuyInterval",
			"Swapping",
		);

		cf_runtime_upgrade_utilities::move_pallet_storage_to::<Pallet<T>>(
			b"CollectedNetworkFee",
			"Swapping",
		);

		Default::default()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, frame_support::sp_runtime::DispatchError> {
		use codec::Encode;
		use frame_support::{ensure, migration};

		for storage in storage_names() {
			ensure!(
				migration::have_storage_value(Pallet::<T>::name().as_bytes(), storage, b""),
				"Storage value not found in LiquidityPools Pallet"
			);
			ensure!(
				!migration::have_storage_value(b"Swapping", storage, b""),
				"Storage value already present in Swapping Pallet"
			);
		}

		let flip_buy_interval = migration::get_storage_value::<
			frame_system::pallet_prelude::BlockNumberFor<T>,
		>(b"LiquidityPools", b"FlipBuyInterval", b"");
		let collected_network_fee = migration::get_storage_value::<cf_primitives::AssetAmount>(
			b"LiquidityPools",
			b"CollectedNetworkFee",
			b"",
		);

		Ok((flip_buy_interval, collected_network_fee).encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), frame_support::sp_runtime::DispatchError> {
		use cf_primitives::AssetAmount;
		use codec::Decode;
		use frame_support::{ensure, migration, sp_runtime::DispatchError};
		use frame_system::pallet_prelude::BlockNumberFor;

		for storage in storage_names() {
			ensure!(
				!migration::have_storage_value(Pallet::<T>::name().as_bytes(), storage, b""),
				"Storage value not removed"
			);
		}

		let (old_flip_buy_interval, old_collected_network_fee) =
			<(Option<BlockNumberFor<T>>, Option<AssetAmount>)>::decode(&mut &state[..])
				.map_err(|_| DispatchError::from("Post upgrade state can't be decoded"))?;

		let (flip_buy_interval, collected_network_fee) = (
			migration::get_storage_value::<frame_system::pallet_prelude::BlockNumberFor<T>>(
				b"Swapping",
				b"FlipBuyInterval",
				b"",
			),
			migration::get_storage_value::<cf_primitives::AssetAmount>(
				b"Swapping",
				b"CollectedNetworkFee",
				b"",
			),
		);

		assert_eq!(old_flip_buy_interval, flip_buy_interval);
		ensure!(old_flip_buy_interval == flip_buy_interval, "FlipBuyInterval doesn't match");
		ensure!(
			old_collected_network_fee == collected_network_fee,
			"CollectedNetworkFee doesn't match"
		);

		Ok(())
	}
}
