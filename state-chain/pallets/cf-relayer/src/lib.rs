#![cfg_attr(not(feature = "std"), no_std)]
use cf_chains::assets::{Asset, AssetAddress};
use codec::{Decode, Encode};
use frame_support::{pallet_prelude::*, sp_runtime::traits::BlockNumberProvider};
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_core::H256;
use sp_std::ops::Add;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weights;
pub use weights::WeightInfo;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy)]
#[scale_info(skip_type_params(T))]
#[codec(mel_bound(T: Config))]
pub struct SwapData<T: Config> {
	trade: (Asset, Asset),
	payout_address: AssetAddress,
	fee: u32,
	block: T::BlockNumber,
	index: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy)]
#[scale_info(skip_type_params(T))]
#[codec(mel_bound(T: Config))]
pub struct SwapIntent<T: Config> {
	swap_data: SwapData<T>,
	tx_hash: H256,
	ingress_address: AssetAddress,
}

#[frame_support::pallet]
pub mod pallet {
	use cf_chains::{
		assets::{AddressDerivation, Asset, AssetAddress},
		eth::{self},
	};
	use cf_traits::VaultAddressProvider;
	use frame_support::StorageHasher;

	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Standard Event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		type EthVaultAddressProvider: VaultAddressProvider<AddressType = eth::Address>;
		type EthAddressDerivation: AddressDerivation<AddressType = eth::Address>;
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::storage]
	/// TODO: write a doc comment
	pub type IndexCounter<T> = StorageValue<_, u32, ValueQuery>;

	/// Map of SwapIntents
	#[pallet::storage]
	#[pallet::getter(fn swap_intents)]
	pub type SwapIntents<T: Config> =
		StorageMap<_, Blake2_128Concat, H256, SwapIntent<T>, OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		NewIngressIntent(AssetAddress, H256),
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(T::WeightInfo::request_swap_intent())]
		pub fn request_swap_intent(
			origin: OriginFor<T>,
			trade: (Asset, Asset),
			payout_address: AssetAddress,
			fee: u32,
		) -> DispatchResultWithPostInfo {
			let _ = ensure_signed(origin)?;

			let next_index = IndexCounter::<T>::get().add(1);
			let swap_data = SwapData {
				trade,
				payout_address,
				fee,
				block: frame_system::Pallet::<T>::current_block_number(),
				index: next_index,
			};

			let tx_hash = H256(Blake2_256::hash(swap_data.encode().as_slice()));

			let vault_address = T::EthVaultAddressProvider::get_vault_address();

			let ingress_address = match trade.0 {
				Asset::EthEth => AssetAddress::ETH(T::EthAddressDerivation::generate_address(
					Asset::EthEth,
					vault_address,
					next_index,
				)),
			};

			SwapIntents::<T>::insert(tx_hash, SwapIntent { swap_data, ingress_address, tx_hash });
			IndexCounter::<T>::put(next_index);
			Self::deposit_event(Event::<T>::NewIngressIntent(ingress_address, tx_hash));
			Ok(().into())
		}
	}
}
