#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

// This should be instatiable to the INCOMING chain.
// This way intents and intent ids align per chain, which makes sense given they act as an index to
// the respective address generation function.

use sp_std::str::FromStr;

use cf_primitives::{ForeignChainAddress, ForeignChainAsset, IntentId};
use cf_traits::{liquidity::LpProvisioningApi, AddressDerivationApi, FlipBalance, IngressApi};

use frame_support::sp_runtime::app_crypto::sp_core::H160;
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

	use frame_system::{ensure_signed, pallet_prelude::OriginFor};

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
		#[pallet::weight(0)]
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

		// TODO: Implement real implementation in liquidity provider pallet
		#[pallet::weight(0)]
		pub fn register_liquidity_ingress_intent_temp(
			origin: OriginFor<T>,
			ingress_asset: ForeignChainAsset,
		) -> DispatchResultWithPostInfo {
			let account_id = ensure_signed(origin)?;

			Self::register_liquidity_ingress_intent(account_id, ingress_asset);

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

pub struct KylesTestnetAddress;

impl AddressDerivationApi for KylesTestnetAddress {
	fn generate_address(
		_ingress_asset: ForeignChainAsset,
		_intent_id: IntentId,
	) -> ForeignChainAddress {
		ForeignChainAddress::Eth(
			H160::from_str("F29aB9EbDb481BE48b80699758e6e9a3DBD609C6")
				.unwrap()
				.to_fixed_bytes(),
		)
	}
}
