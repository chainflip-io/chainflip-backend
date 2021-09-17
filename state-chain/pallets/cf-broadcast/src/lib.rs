#![cfg_attr(not(feature = "std"), no_std)]
// This can be removed after rustc version 1.53.
#![feature(int_bits_const)]

//! Transaction Broadcast Pallet
//! https://swimlanes.io/d/DJNaWp1Go

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

use cf_chains::Chain;
use cf_traits::{Chainflip, SignerNomination};
use codec::{Decode, Encode};
use frame_support::Parameter;
use frame_system::pallet_prelude::OriginFor;
pub use pallet::*;
use sp_std::marker::PhantomData;
use sp_std::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum BroadcastFailure {
	/// The transaction was rejected because of some user error.
	TransactionRejected,
	/// The transaction failed for some unknown reason.
	TransactionFailed,
	/// The transaction stalled.
	TransactionTimeout,
}

/// The [TransactionContext] should contain all the state required to construct and process transactions for a given
/// chain.
pub trait BroadcastConfig<T: Chainflip> {
	/// A chain identifier.
	type Chain: Chain;
	/// An unsigned version of the transaction that needs to signed before it can be broadcast.
	type UnsignedTransaction: Parameter;
	/// A transaction that has been signed by some account and is ready to be broadcast.
	type SignedTransaction: Parameter;
	/// The transaction hash type used to uniquely identify signed transactions.
	type TransactionHash: Parameter;

	/// Verify the signed transaction when it is submitted to the state chain by the nominated signer.
	fn verify_transaction(
		signer: &T::ValidatorId,
		unsigned_tx: &Self::UnsignedTransaction,
		signed_tx: &Self::SignedTransaction,
	) -> Option<()>;
}

pub type BroadcastId = u64;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{ensure, pallet_prelude::*};
	use frame_support::{dispatch::DispatchResultWithPostInfo, Twox64Concat};
	use frame_system::pallet_prelude::*;

	pub type SignedTransactionFor<T, I> =
		<<T as Config<I>>::BroadcastConfig as BroadcastConfig<T>>::SignedTransaction;
	pub type UnsignedTransactionFor<T, I> =
		<<T as Config<I>>::BroadcastConfig as BroadcastConfig<T>>::UnsignedTransaction;
	pub type TransactionHashFor<T, I> =
		<<T as Config<I>>::BroadcastConfig as BroadcastConfig<T>>::TransactionHash;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config<I: 'static = ()>: Chainflip {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self, I>> + IsType<<Self as frame_system::Config>::Event>;

		/// A marker trait identifying the chain that we are broadcasting to.
		type TargetChain: Chain;

		/// The broadcast configuration for this instance.
		type BroadcastConfig: BroadcastConfig<Self, Chain = Self::TargetChain>;

		/// Signer nomination.
		type SignerNomination: SignerNomination<SignerId = Self::ValidatorId>;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	#[pallet::storage]
	#[pallet::getter(fn request_id_counter)]
	pub type BroadcastIdCounter<T, I = ()> = StorageValue<_, BroadcastId, ValueQuery>;

	#[pallet::storage]
	pub type AwaitingSignature<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Twox64Concat,
		BroadcastId,
		(T::ValidatorId, UnsignedTransactionFor<T, I>),
		OptionQuery,
	>;

	#[pallet::storage]
	pub type AwaitingBroadcast<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BroadcastId, SignedTransactionFor<T, I>, OptionQuery>;

	#[pallet::storage]
	pub type RetryQueue<T: Config<I>, I: 'static = ()> =
		StorageValue<_, Vec<BroadcastId>, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// [broadcast_id, validator_id, unsigned_tx]
		TransactionSigningRequest(BroadcastId, T::ValidatorId, UnsignedTransactionFor<T, I>),
		/// [broadcast_id, signed_tx]
		BroadcastRequest(BroadcastId, SignedTransactionFor<T, I>),
		/// [broadcast_id]
		BroadcastComplete(BroadcastId),
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// The provided request id is invalid.
		InvalidBroadcastId,
		/// The transaction signer is not signer who was nominated.
		InvalidSigner,
	}

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I> {}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Begin the process of broadcasting a transaction.
		///
		/// This is the first step - requsting a transaction signature from a nominated validator.
		#[pallet::weight(10_000)]
		pub fn start_broadcast(
			origin: OriginFor<T>,
			unsigned_tx: UnsignedTransactionFor<T, I>,
		) -> DispatchResultWithPostInfo {
			let _ = T::EnsureWitnessed::ensure_origin(origin)?;

			// Get a new id.
			let id = BroadcastIdCounter::<T, I>::mutate(|id| {
				*id += 1;
				*id
			});

			// Select a signer for this broadcast.
			let nominated_signer = T::SignerNomination::nomination_with_seed(id);

			AwaitingSignature::<T, I>::insert(id, (nominated_signer.clone(), unsigned_tx.clone()));

			// Emit the transaction signing request.
			Self::deposit_event(Event::<T, I>::TransactionSigningRequest(
				id,
				nominated_signer,
				unsigned_tx,
			));

			Ok(().into())
		}

		/// Called when the transaction is ready to be broadcast. The signed transaction is stored on-chain so that
		/// any node can potentially broadcast it to the target chain.
		#[pallet::weight(10_000)]
		pub fn transaction_ready(
			origin: OriginFor<T>,
			id: BroadcastId,
			signed_tx: SignedTransactionFor<T, I>,
		) -> DispatchResultWithPostInfo {
			let signer = ensure_signed(origin)?;

			let (nominated_signer, unsigned_tx) =
				AwaitingSignature::<T, I>::get(id).ok_or(Error::<T, I>::InvalidBroadcastId)?;

			ensure!(
				nominated_signer.into() == signer,
				Error::<T, I>::InvalidSigner
			);

			AwaitingSignature::<T, I>::remove(id);

			if T::BroadcastConfig::verify_transaction(&signer.into(), &unsigned_tx, &signed_tx)
				.is_some()
			{
				Self::deposit_event(Event::<T, I>::BroadcastRequest(id, signed_tx.clone()));
				AwaitingBroadcast::<T, I>::insert(id, signed_tx);
			} else {
				// the authored transaction is invalid.
				// punish the signer and retry.
				todo!()
			}

			Ok(().into())
		}

		/// Nodes have witnessed that the transaction has reached finality on the target chain.
		#[pallet::weight(10_000)]
		pub fn broadcast_success(
			origin: OriginFor<T>,
			id: BroadcastId,
			_tx_hash: TransactionHashFor<T, I>,
		) -> DispatchResultWithPostInfo {
			let _ = T::EnsureWitnessed::ensure_origin(origin)?;

			ensure!(
				AwaitingBroadcast::<T, I>::contains_key(id),
				Error::<T, I>::InvalidBroadcastId
			);

			// Remove the broadcast now it's completed.
			AwaitingBroadcast::<T, I>::remove(id);
			Self::deposit_event(Event::<T, I>::BroadcastComplete(id));

			Ok(().into())
		}

		/// Nodes have witnessed that something went wrong. The transaction may have been rejected outright or may
		/// have stalled on the target chain.
		#[pallet::weight(10_000)]
		pub fn broadcast_failure(
			origin: OriginFor<T>,
			id: BroadcastId,
			failure: BroadcastFailure,
			_tx_hash: TransactionHashFor<T, I>,
		) -> DispatchResultWithPostInfo {
			let _ = T::EnsureWitnessed::ensure_origin(origin)?;

			let _signed_tx = AwaitingBroadcast::<T, I>::take(id)
				.ok_or(Error::<T, I>::InvalidBroadcastId)?;

			match failure {
				BroadcastFailure::TransactionRejected => {
					// Report and nominate a new signer. Retry.
					todo!()
				}
				BroadcastFailure::TransactionTimeout => {
					// Nominate a new signer, but don't report the old one. Retry.
					todo!()
				}
				BroadcastFailure::TransactionFailed => {
					// This is bad.
					todo!()
				}
			};

			Ok(().into())
		}
	}
}
