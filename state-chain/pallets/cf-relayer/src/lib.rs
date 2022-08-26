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
pub struct SwapData {
	trade: (Chain, Chain),
	payout_address: ChainAddress,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy)]
#[scale_info(skip_type_params(T))]
#[codec(mel_bound(T: Config))]
pub struct SwapIntent<T: Config> {
	swap_data: SwapData,
	fee: u32,
	tx_hash: H256,
	block: T::BlockNumber,
	index: u64,
}

#[frame_support::pallet]
pub mod pallet {
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
			swap_data: SwapData,
		) -> DispatchResultWithPostInfo {
			let relayer = ensure_signed(origin)?;
			ensure!(Relayers::<T>::get().contains(&relayer), Error::<T>::CallerIsNoRelayer);
			let next_index = IndexCounter::<T>::get().add(1);
			let swap_intent = SwapIntent {
				swap_data,
				fee: 1,
				tx_hash: H256::default(),
				block: frame_system::Pallet::<T>::current_block_number(),
				index: next_index,
			};
			let ingress_address = Self::derive_ingress_address(swap_intent.clone());
			Self::deposit_event(Event::<T>::NewSwapIntent(ingress_address, swap_intent.tx_hash));
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
		fn derive_ingress_address(swap_intent: SwapIntent<T>) -> ChainAddress {
			match swap_intent.swap_data.trade.0 {
				ETH => ChainAddress::ETH(eth::Address::default()),
			}
		}
	}
}
