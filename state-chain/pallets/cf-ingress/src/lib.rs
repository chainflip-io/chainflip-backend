#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

// This should be instatiable to the INCOMING chain.
// This way intents and intent ids align per chain, which makes sense given they act as an index to
// the respective address generation function.

use cf_primitives::{ForeignChainAddress, ForeignChainAsset, IntentId};
use cf_traits::{liquidity::LpProvisioningApi, AddressDerivationApi, FlipBalance, IngressApi};

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weights;
pub use weights::WeightInfo;

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use core::marker::PhantomData;

	use super::*;
	use cf_primitives::Asset;
	use frame_support::{
		pallet_prelude::{DispatchResultWithPostInfo, OptionQuery, ValueQuery, *},
		traits::{EnsureOrigin, IsType},
		Blake2_128,
	};

	use frame_system::pallet_prelude::OriginFor;

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
		Blake2_128,
		ForeignChainAddress,
		Intent<<T as frame_system::Config>::AccountId>,
		OptionQuery,
	>;

	/// Stores the latest intent id used to generate an address.
	#[pallet::storage]
	pub type IntentIdCounter<T: Config> = StorageValue<_, IntentId, ValueQuery>;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: cf_traits::Chainflip {
		/// Standard Event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		/// Generates ingress addresses.
		type AddressDerivation: AddressDerivationApi;
		/// Pallet responsible for managing Liquidity Providers.
		type LpAccountHandler: LpProvisioningApi<AccountId = Self::AccountId, Amount = FlipBalance>;
		/// Benchmark weights
		type WeightInfo: WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		// We only want to witness for one asset on a particular chain
		StartWitnessing { ingress_address: ForeignChainAddress, ingress_asset: ForeignChainAsset },

		IngressCompleted { ingress_address: ForeignChainAddress, asset: Asset, amount: u128 },
	}

	#[pallet::error]
	pub enum Error<T> {
		InvalidIntent,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(T::WeightInfo::do_ingress())]
		pub fn do_ingress(
			origin: OriginFor<T>,
			ingress_address: ForeignChainAddress,
			asset: Asset,
			amount: u128,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;

			// NB: Don't take here. We should continue witnessing this address
			// even after an ingress to it has occurred.
			// https://github.com/chainflip-io/chainflip-eth-contracts/pull/226
			match OpenIntents::<T>::get(ingress_address).ok_or(Error::<T>::InvalidIntent)? {
				Intent::LiquidityProvision { lp_account, .. } => {
					T::LpAccountHandler::provision_account(&lp_account, asset, amount)?;
				},
				Intent::Swap { .. } => todo!(),
			}

			Self::deposit_event(Event::IngressCompleted { ingress_address, asset, amount });

			Ok(().into())
		}
	}
}

impl<T: Config> Pallet<T> {
	fn generate_new_address(ingress_asset: ForeignChainAsset) -> (IntentId, ForeignChainAddress) {
		let intent_id = IntentIdCounter::<T>::mutate(|id| {
			*id += 1;
			*id
		});
		let ingress_address = T::AddressDerivation::generate_address(ingress_asset, intent_id);
		(intent_id, ingress_address)
	}
}

impl<T: Config> IngressApi for Pallet<T> {
	type AccountId = <T as frame_system::Config>::AccountId;

	// This should be callable by the LP pallet.
	fn register_liquidity_ingress_intent(
		lp_account: Self::AccountId,
		ingress_asset: ForeignChainAsset,
	) -> (IntentId, ForeignChainAddress) {
		let (intent_id, ingress_address) = Self::generate_new_address(ingress_asset);

		OpenIntents::<T>::insert(
			ingress_address,
			Intent::LiquidityProvision {
				lp_account,
				ingress_details: IngressDetails { intent_id, ingress_asset },
			},
		);

		Self::deposit_event(Event::StartWitnessing { ingress_address, ingress_asset });

		(intent_id, ingress_address)
	}

	// This should only be callable by the relayer.
	fn register_swap_intent(
		ingress_asset: ForeignChainAsset,
		egress_asset: ForeignChainAsset,
		egress_address: ForeignChainAddress,
		relayer_commission_bps: u16,
	) -> (IntentId, ForeignChainAddress) {
		let (intent_id, ingress_address) = Self::generate_new_address(ingress_asset);

		OpenIntents::<T>::insert(
			ingress_address,
			Intent::Swap {
				ingress_details: IngressDetails { intent_id, ingress_asset },
				egress_address,
				egress_asset,
				relayer_commission_bps,
			},
		);

		Self::deposit_event(Event::StartWitnessing { ingress_address, ingress_asset });

		(intent_id, ingress_address)
	}
}
