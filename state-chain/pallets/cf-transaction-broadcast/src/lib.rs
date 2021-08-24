#![cfg_attr(not(feature = "std"), no_std)]

//! Transaction Broadcast Pallet

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

use codec::{Decode, Encode};

use frame_support::{
	dispatch::{DispatchResultWithPostInfo, Dispatchable, PostDispatchInfo},
	Parameter,
};
use frame_system::pallet_prelude::OriginFor;
pub use pallet::*;
use sp_runtime::RuntimeDebug;
use sp_std::marker::PhantomData;
use sp_std::prelude::*;

pub trait BaseConfig: frame_system::Config + std::fmt::Debug {
	/// The id type used to identify individual signing keys.
	type KeyId: Parameter;
	type ValidatorId: Parameter;
	type ChainId: Parameter;
}

pub enum ChainId {
	Eth,
	Btc,
	Dot,
}

pub trait BroadcastContext {
	const CHAIN_ID: ChainId;

	type Payload: Parameter;
	type Signature: Parameter;
	type UnsignedTransaction: Parameter;
	type SignedTransaction: Parameter;

	fn construct_signing_payload(&self) -> Self::Payload;
}

// These would be defined in their own modules but adding it here for now.
// Macros might help reduce the boilerplat but I don't think it's too bad.
pub mod instances {
	pub use super::*;
	use codec::{Decode, Encode};

	// A signature request.
	pub mod eth {
		use super::*;
		use sp_core::H256;
		
		#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Encode, Decode)]
		struct EthBroadcastContext;

		impl BroadcastContext for EthBroadcastContext {
			const CHAIN_ID: ChainId = ChainId::Eth;

			type Payload = H256;
			type Signature = H256;
			type UnsignedTransaction = ();
			type SignedTransaction = ();

			fn construct_signing_payload(&self) -> Self::Payload {
					todo!()
			}
		}
	}
}

pub type BroadcastId = u64;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use codec::FullCodec;
	use frame_support::pallet_prelude::*;
	use frame_support::{dispatch::DispatchResultWithPostInfo, Twox64Concat};
	use frame_system::pallet_prelude::*;

	type PayloadFor<T, I> = <<T as Config<I>>::BroadcastContext as BroadcastContext>::Payload;
	type SignatureFor<T, I> = <<T as Config<I>>::BroadcastContext as BroadcastContext>::Signature;
	type SignedTransactionFor<T, I> = <<T as Config<I>>::BroadcastContext as BroadcastContext>::SignedTransaction;
	type UnsignedTransactionFor<T, I> = <<T as Config<I>>::BroadcastContext as BroadcastContext>::UnsignedTransaction;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config<I: 'static = ()>: BaseConfig {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self, I>> + IsType<<Self as frame_system::Config>::Event>;

		/// A type that allows us to check if a call was a result of witness consensus.
		type EnsureWitnessed: EnsureOrigin<Self::Origin>;

		/// The context definition for this instance.
		type BroadcastContext: BroadcastContext + Member + FullCodec;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	#[pallet::storage]
	#[pallet::getter(fn request_id_counter)]
	pub type BroadcastIdCounter<T, I = ()> = StorageValue<_, BroadcastId, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn pending_request)]
	pub type PendingBroadcasts<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BroadcastId, T::BroadcastContext, OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		///
		ThresholdSignatureRequest(BroadcastId, PayloadFor<T, I>),
		///
		TransactionSigningRequest(BroadcastId, T::AccountId, UnsignedTransactionFor<T, I>),
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// The provided request id is invalid.
		InvalidBroadcastId,
	}

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I> {}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		///
		#[pallet::weight(10_000)]
		pub fn signature_ready(
			origin: OriginFor<T>,
			id: BroadcastId,
			signature: SignatureFor<T, I>,
		) -> DispatchResultWithPostInfo {
			let _ = T::EnsureWitnessed::ensure_origin(origin.clone())?;

			ensure!(
				PendingBroadcasts::<T, I>::contains_key(id), 
				Error::<T, I>::InvalidBroadcastId
			);

			// Update the context.
			let unsigned_tx = PendingBroadcasts::<T, I>::mutate(id, |ctx| {
				todo!("signature ready, construct the transaction")
			});

			// Select a signer.
			let signer = todo!();

			Self::deposit_event(Event::<T, I>::TransactionSigningRequest(id, signer, unsigned_tx));

			Ok(().into())
		}

		///
		#[pallet::weight(10_000)]
		pub fn transaction_ready(
			origin: OriginFor<T>,
			id: BroadcastId,
			signed_tx: SignedTransactionFor<T, I>,
		) -> DispatchResultWithPostInfo {
			let _ = T::EnsureWitnessed::ensure_origin(origin.clone())?;

			ensure!(
				PendingBroadcasts::<T, I>::contains_key(id),
				Error::<T, I>::InvalidBroadcastId
			);

			// Update the context.
			let context = PendingBroadcasts::<T, I>::mutate(id, |ctx| {
				todo!("Update the state of the transaction: ready to be broadcast.")
			});

			// TODO: Store the transaction or emit the event?
			Ok(().into())
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	/// Initiates a broadcast and returns its id.
	pub fn initiate_broadcast(context: T::BroadcastContext) -> u64 {
		// Get a new id.
		let id = BroadcastIdCounter::<T, I>::mutate(|id| {
			*id += 1;
			*id
		});

		// Store the context.
		PendingBroadcasts::<T, I>::insert(id, &context);

		// Emit the initial request to the CFE.
		Self::deposit_event(Event::<T, I>::ThresholdSignatureRequest(id, context.construct_signing_payload()));

		id
	}
}
