#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

// This should be instatiable to the INCOMING chain.
// This way intents and intent ids align per chain, which makes sense given they act as an index to
// the respective address generation function.

use cf_primitives::{ForeignChainAddress, ForeignChainAsset};
use cf_traits::IngressApi;

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use core::marker::PhantomData;

	use super::*;
	use cf_primitives::{Asset, ForeignChain};
	use frame_support::{
		pallet_prelude::{DispatchResultWithPostInfo, OptionQuery, ValueQuery, *},
		sp_runtime::app_crypto::sp_core::H160,
		sp_std,
		traits::{EnsureOrigin, IsType},
		Twox64Concat,
	};

	use frame_system::{ensure_signed, pallet_prelude::OriginFor};

	use sp_std::str::FromStr;

	type IntentId = u64;

	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub struct IngressDetails {
		pub intent_id: IntentId,
		pub ingress_asset: ForeignChainAsset,
	}

	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub enum Intent<AccountId> {
		Swap {
			ingress_details: IngressDetails,
			egress_asset: ForeignChainAsset,
			egress_address: ForeignChainAddress,
			relayer_commission_bps: u16,
		},
		LiquidityProvision {
			ingress_details: IngressDetails,
			lp_account: AccountId,
		},
	}

	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::generate_store(pub (super) trait Store)]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::storage]
	pub type OpenIntents<T: Config> = StorageMap<
		_,
		Twox64Concat,
		ForeignChainAddress,
		Intent<<T as frame_system::Config>::AccountId>,
		OptionQuery,
	>;

	/// Stores the latest intent id returned for a particular foreign chain.
	#[pallet::storage]
	pub type IntentIdCounter<T: Config> =
		StorageMap<_, Twox64Concat, ForeignChain, IntentId, ValueQuery>;

	/// Temp storage item to allow dummy StartWitnessing requests
	#[pallet::storage]
	pub type DummyStartWitnessing<T: Config> =
		StorageValue<_, (ForeignChainAddress, Asset), OptionQuery>;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: cf_traits::Chainflip {
		/// Standard Event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub start_witnessing_address: ForeignChainAddress,
		pub ingress_asset: Asset,
		pub _config: PhantomData<T>,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				// Kyle's testnet address
				start_witnessing_address: ForeignChainAddress::Eth(
					H160::from_str("F29aB9EbDb481BE48b80699758e6e9a3DBD609C6").unwrap(),
				),
				ingress_asset: Asset::Eth,
				_config: PhantomData,
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			DummyStartWitnessing::<T>::put((self.start_witnessing_address, self.ingress_asset));
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		// We only want to witness for one asset on a particular chain
		StartWitnessing { address: ForeignChainAddress, ingress_asset: Asset },

		IngressCompleted { address: ForeignChainAddress, asset: Asset, amount: u128 },
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(0)]
		pub fn do_ingress(
			origin: OriginFor<T>,
			address: ForeignChainAddress,
			asset: Asset,
			amount: u128,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;

			// TODO: Match these address, assets and amounts to their intent ids
			// and perform relevant actions

			Self::deposit_event(Event::IngressCompleted { address, asset, amount });
			Ok(().into())
		}

		// Temp dummy extrinsics to allow me to create StartWitnessing events on demand
		#[pallet::weight(0)]
		pub fn emit_dummy_event(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			ensure_signed(origin)?;
			Self::dummy_emit_start_witnessing();
			Ok(().into())
		}
	}
}

impl<T: Config> Pallet<T> {
	fn dummy_emit_start_witnessing() {
		let (address, ingress_asset) =
			DummyStartWitnessing::<T>::get().expect("Inserted at genesis");
		Self::deposit_event(Event::StartWitnessing { address, ingress_asset });
	}
}

impl<T: Config> IngressApi for Pallet<T> {
	type AccountId = <T as frame_system::Config>::AccountId;

	// This should be callable by the LP pallet
	fn register_liquidity_ingress_intent(
		_lp_account: Self::AccountId,
		_ingress_asset: ForeignChainAsset,
	) {
		Self::dummy_emit_start_witnessing()
	}

	// This should only be callable by the relayer.
	fn register_swap_intent(
		_relayer_id: Self::AccountId,
		_ingress_asset: ForeignChainAsset,
		_egress_asset: ForeignChainAsset,
		_egress_address: ForeignChainAddress,
		_relayer_commission_bps: u16,
	) {
		Self::dummy_emit_start_witnessing()
	}
}
