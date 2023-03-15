#![cfg_attr(not(feature = "std"), no_std)]
#![feature(drain_filter)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

use cf_primitives::{Asset, AssetAmount, ForeignChainAddress, ForeignChain};
use cf_traits::{AccountRoleRegistry, SwappingApi};
use frame_support::pallet_prelude::*;
pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use core::marker::PhantomData;
	use sp_std::vec::Vec;

	use cf_traits::Chainflip;
	use frame_system::pallet_prelude::OriginFor;

	/// Stores how much gas fee has been accumulated for each asset.
	#[pallet::storage]
	pub type GasFeeAccumulated<T: Config> =
		StorageMap<_, Identity, Asset, AssetAmount, ValueQuery>;

	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::generate_store(pub (super) trait Store)]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// For registering and verifying the account role.
		type AccountRoleRegistry: AccountRoleRegistry<Self>;

		/// An interface to the AMM api implementation.
		type SwappingApi: SwappingApi;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		CcmEgressScheduled{
			ingress_asset: Asset,
			ingress_amount: AssetAmount,
			egress_asset: Asset,
			egress_amount: AssetAmount,
			egress_address: ForeignChainAddress,
			output_gas_asset: Asset,
			output_gas_amount: AssetAmount,
			message: Vec<u8>,
			refund_address: ForeignChainAddress,
		}
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
		/// Register a new Cross-chain-message. The fund is swapped into the target
		/// chain's native asset, with appropriate fees and gas deducted, and the 
		/// message is egressed to the target chain.
		/// 
		/// ## Events
		///
		/// - [NewCcmIntent](Event::NewCcmIntent)
		#[pallet::weight(0)]
		pub fn send_ccm(
			origin: OriginFor<T>,
			ingress_asset: Asset,
			ingress_amount: AssetAmount,
			egress_asset: Asset,
			egress_address: ForeignChainAddress,
			egress_gas_amount: AssetAmount,
			message: Vec<u8>,
			refund_address: ForeignChainAddress,
		) -> DispatchResult {
			// witness origin
			let relayer = T::AccountRoleRegistry::ensure_relayer(origin)?;
			
			ensure!(ForeignChain::from(egress_address) == ForeignChain::from(refund_address), Error::<T>::IncompatibleEgressAndReturnAddress);
			ensure!(ForeignChain::from(egress_address) == ForeignChain::from(egress_asset), Error::<T>::IncompatibleEgressAndReturnAddress);

			// Swap all funds into egress chain's native currency.
			let swapped_output = T::SwappingApi::swap(ingress_asset, egress_asset, ingress_amount)?;
			
			// Extract gas from the swap output.
			let output_gas_asset = ForeignChain::from(egress_asset).gas_asset();

			// schedule egress that contains the message and refund address
			let (egress_amount, output_gas_amount) = if egress_asset != output_gas_asset {
				// Gas asset is different from the egress asset. Another swap is required.
				

				// Calculate input required for the gas amount

				// perform the swap

				Ok((0, 0))
			} else {
				// Split the gas amount from the egress amount.
				let remaining = swapped_output.checked_sub(egress_gas_amount).ok_or(Error::<T>::InsufficientBalanceForGas)?;
				Ok((remaining, egress_gas_amount))
			}?;


			Self::deposit_event(Event::<T>::CcmEgressScheduled {
				ingress_asset,
				ingress_amount,
				egress_asset,
				egress_amount,
				egress_address,
				output_gas_asset,
				output_gas_amount,
				message,
				refund_address,
			});

			Ok(())
		}
	}
}