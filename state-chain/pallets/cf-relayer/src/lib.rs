use cf_chains::eth;
use codec::{Decode, Encode};
use frame_support::{pallet_prelude::*, sp_runtime::traits::BlockNumberProvider};
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_core::H256;
use sp_std::ops::Add;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy)]
pub enum Chain {
	ETH,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy)]
pub enum ChainAddress {
	ETH(eth::Address),
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy)]
#[scale_info(skip_type_params(T))]
#[codec(mel_bound(T: Config))]
pub struct SwapData<T: Config> {
	trade: (Chain, Chain),
	payout_address: ChainAddress,
	fee: u32,
	block: T::BlockNumber,
	index: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy)]
#[scale_info(skip_type_params(T))]
#[codec(mel_bound(T: Config))]
pub struct SwapIntent<T: Config> {
	swap_data: SwapData<T>,
	tx_hash: H256,
	ingress_address: ChainAddress,
}

#[frame_support::pallet]
pub mod pallet {
	use frame_support::StorageHasher;

	use super::*;
	use crate::Chain::ETH;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
	}

	#[pallet::error]
	pub enum Error<T> {
		CallerIsNoRelayer,
	}

	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::storage]
	/// TODO: write a doc comment
	pub type IndexCounter<T> = StorageValue<_, u64, ValueQuery>;

	/// TODO: nice comment needed
	#[pallet::storage]
	#[pallet::getter(fn swap_intents)]
	pub type SwapIntents<T: Config> =
		StorageMap<_, Blake2_128Concat, H256, SwapIntent<T>, OptionQuery>;

	/// TODO: nice comment needed
	#[pallet::storage]
	#[pallet::getter(fn relayers)]
	pub type Relayers<T: Config> = StorageValue<_, Vec<T::AccountId>, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		NewSwapIntent(ChainAddress, H256),
		NewRelayer(T::AccountId),
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(10_000)]
		pub fn request_swap_intent(
			origin: OriginFor<T>,
			trade: (Chain, Chain),
			payout_address: ChainAddress,
			fee: u32,
		) -> DispatchResultWithPostInfo {
			let relayer = ensure_signed(origin)?;
			ensure!(Relayers::<T>::get().contains(&relayer), Error::<T>::CallerIsNoRelayer);
			let next_index = IndexCounter::<T>::get().add(1);
			let swap_data = SwapData {
				trade,
				payout_address,
				fee,
				block: frame_system::Pallet::<T>::current_block_number(),
				index: next_index,
			};
			let tx_hash = H256(Blake2_256::hash(swap_data.encode().as_slice()));
			let ingress_address = Self::derive_ingress_address(swap_data.clone());
			let swap_intent = SwapIntent { swap_data, ingress_address, tx_hash };
			SwapIntents::<T>::insert(tx_hash, swap_intent);
			IndexCounter::<T>::put(next_index);
			Self::deposit_event(Event::<T>::NewSwapIntent(ingress_address, tx_hash));
			Ok(().into())
		}
		#[pallet::weight(10_000)]
		pub fn register(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let relayer = ensure_signed(origin)?;
			Relayers::<T>::append(relayer.clone());
			Self::deposit_event(Event::<T>::NewRelayer(relayer));
			Ok(().into())
		}
	}

	impl<T: Config> Pallet<T> {
		fn derive_ingress_address(swap_intent: SwapData<T>) -> ChainAddress {
			match swap_intent.trade.0 {
				ETH => ChainAddress::ETH(eth::Address::default()),
			}
		}
	}
}
