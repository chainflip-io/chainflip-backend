#![cfg_attr(not(feature = "std"), no_std)]
#![feature(drain_filter)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

use cf_primitives::{
	chains::AnyChain, Asset, AssetAmount, CcmIngressMetadata, ForeignChain, ForeignChainAddress,
};
use cf_traits::{CcmHandler, SwappingApi};
use frame_support::pallet_prelude::*;
pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use core::marker::PhantomData;
	use sp_std::vec::Vec;

	use cf_traits::{Chainflip, EgressApi};
	use frame_system::pallet_prelude::OriginFor;

	/// Stores how much gas fee has been accumulated for each asset.
	#[pallet::storage]
	pub type GasFeeAccumulated<T: Config> = StorageMap<_, Identity, Asset, AssetAmount, ValueQuery>;

	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::generate_store(pub (super) trait Store)]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// An interface to the AMM api implementation.
		type SwappingApi: SwappingApi;

		/// API for handling asset egress.
		type EgressHandler: EgressApi<AnyChain>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		CcmEgressScheduled {
			ingress_asset: Asset,
			ingress_amount: AssetAmount,
			caller_address: ForeignChainAddress,
			egress_asset: Asset,
			egress_amount: AssetAmount,
			egress_address: ForeignChainAddress,
			output_gas_asset: Asset,
			output_gas_amount: AssetAmount,
			message: Vec<u8>,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		// The Egress and return address must be from the same chain.
		IncompatibleEgressAndReturnAddress,
		// The ingress amount is insufficient to pay for the gas.
		InsufficientBalanceForGas,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Process the ingress of a Cross-chain-message. The fund is swapped into the target
		/// chain's native asset, with appropriate fees and gas deducted, and the
		/// message is egressed to the target chain.
		///
		/// ## Events
		///
		/// - [NewCcmIntent](Event::NewCcmIntent)
		#[pallet::weight(0)]
		pub fn ccm_ingress(
			origin: OriginFor<T>,
			ingress_asset: Asset,
			ingress_amount: AssetAmount,
			egress_asset: Asset,
			egress_address: ForeignChainAddress,
			message_metadata: CcmIngressMetadata,
		) -> DispatchResult {
			T::EnsureWitnessed::ensure_origin(origin)?;

			Self::on_ccm_ingress(
				ingress_asset,
				ingress_amount,
				egress_asset,
				egress_address,
				message_metadata,
			)
		}
	}

	impl<T: Config> CcmHandler for Pallet<T> {
		fn on_ccm_ingress(
			ingress_asset: Asset,
			ingress_amount: AssetAmount,
			egress_asset: Asset,
			egress_address: ForeignChainAddress,
			message_metadata: CcmIngressMetadata,
		) -> DispatchResult {
			ensure!(
				ForeignChain::from(egress_address.clone()) == ForeignChain::from(egress_asset),
				Error::<T>::IncompatibleEgressAndReturnAddress
			);

			// Swap all funds into egress chain's native currency.
			let swapped_output = T::SwappingApi::swap(ingress_asset, egress_asset, ingress_amount)?;

			let output_gas_asset = ForeignChain::from(egress_asset).gas_asset();
			// Calculate expected amount of gas from target chain's current gas price.
			// TODO: Add gas query from chian-tracking trait/pallet
			let expected_output_gas = message_metadata.gas_budget;

			// schedule egress that contains the message and refund address
			let (egress_amount, output_gas_amount) = if egress_asset != output_gas_asset {
				// Gas asset is different from the egress asset. Another swap is required.
				// Calculate input required for the gas amount
				// TODO add interface to estimate input required for an output amount

				// TODO: perform the swap
				(0, 0)
			} else {
				// Split the gas amount from the egress amount.
				let remaining = swapped_output.saturating_sub(expected_output_gas);
				(remaining, expected_output_gas)
			};

			// Send CCM via egress
			let _egress_id = T::EgressHandler::schedule_egress(
				egress_asset,
				egress_amount,
				egress_address.clone(),
				Some(message_metadata.clone()),
			);

			// TODO: Store swapped gas with a generated EgressId.

			// Deposit event
			Self::deposit_event(Event::<T>::CcmEgressScheduled {
				ingress_asset,
				ingress_amount,
				caller_address: message_metadata.caller_address,
				egress_asset,
				egress_amount,
				egress_address,
				output_gas_asset,
				output_gas_amount,
				message: message_metadata.message,
			});

			Ok(())
		}
	}
}
