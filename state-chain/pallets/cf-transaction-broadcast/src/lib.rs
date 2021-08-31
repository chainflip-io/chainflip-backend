#![cfg_attr(not(feature = "std"), no_std)]
// This can be removed after rustc version 1.53.
#![feature(int_bits_const)]

//! Transaction Broadcast Pallet

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod instances;

use frame_support::Parameter;
use frame_system::pallet_prelude::OriginFor;
pub use pallet::*;
use sp_std::prelude::*;
use sp_std::{cmp::min, marker::PhantomData, mem::size_of};
use codec::{Decode, Encode};

pub trait BaseConfig: frame_system::Config + std::fmt::Debug {
	/// The id type used to identify individual signing keys.
	type KeyId: Parameter;
	type ValidatorId: Parameter;
	type ChainId: Parameter;
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum BroadcastFailure<SignerId: Parameter> {
	/// The threshold signature step failed, we need to blacklist some nodes.
	MpcFailure {
		bad_nodes: Vec<SignerId>,
	},
	/// The transaction was rejected.
	TransactionFailure,
	/// The transaction stalled.
	TransactionTimeout,
}

pub trait BroadcastContext<T: BaseConfig> {
	type Payload: Parameter;
	type Signature: Parameter;
	type UnsignedTransaction: Parameter;
	type SignedTransaction: Parameter;
	type TransactionHash: Parameter;

	/// Constructs the payload for the threshold signature.
	fn construct_signing_payload(&mut self) -> Self::Payload;

	/// Constructs the outgoing transaction using the payload signature.
	fn construct_unsigned_transaction(
		&mut self,
		sig: &Self::Signature,
	) -> Self::UnsignedTransaction;

	/// Optional callback for when the signed transaction is submitted to the state chain.
	fn on_transaction_ready(&mut self, signed_tx: &Self::SignedTransaction) {}

	/// Optional callback for when a transaction has been witnessed on the host chain.
	fn on_broadcast_success(&mut self, transaction_hash: &Self::TransactionHash) {}

	/// Optional callback for when a transaction has failed.
	fn on_broadcast_failure(&mut self, failure: &BroadcastFailure<T::ValidatorId>) {}
}

/// Something that can nominate signers from the set of active validators.
pub trait SignerNomination {
	/// The id type of signers. Most likely the same as the runtime's `ValidatorId`.
	type SignerId;

	/// Returns a random live signer. The seed value is used as a source of randomness.
	fn nomination_with_seed(seed: u64) -> Self::SignerId;

	/// Returns a list of live signers where the number of signers is sufficient to author a threshold signature. The
	/// seed value is used as a source of randomness.
	fn threshold_nomination_with_seed(seed: u64) -> Vec<Self::SignerId>;
}

pub type BroadcastId = u64;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use codec::FullCodec;
	use frame_support::pallet_prelude::*;
	use frame_support::{dispatch::DispatchResultWithPostInfo, Twox64Concat};
	use frame_system::pallet_prelude::*;

	pub type PayloadFor<T, I> = <<T as Config<I>>::BroadcastContext as BroadcastContext<T>>::Payload;
	pub type SignatureFor<T, I> = <<T as Config<I>>::BroadcastContext as BroadcastContext<T>>::Signature;
	pub type SignedTransactionFor<T, I> = <<T as Config<I>>::BroadcastContext as BroadcastContext<T>>::SignedTransaction;
	pub type UnsignedTransactionFor<T, I> = <<T as Config<I>>::BroadcastContext as BroadcastContext<T>>::UnsignedTransaction;
	pub type TransactionHashFor<T, I> = <<T as Config<I>>::BroadcastContext as BroadcastContext<T>>::TransactionHash;
	pub type BroadcastFailureFor<T> = BroadcastFailure<T>;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config<I: 'static = ()>: BaseConfig {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self, I>> + IsType<<Self as frame_system::Config>::Event>;

		/// A type that allows us to check if a call was a result of witness consensus.
		type EnsureWitnessed: EnsureOrigin<Self::Origin>;

		/// The context definition for this instance.
		type BroadcastContext: BroadcastContext<Self> + Member + FullCodec;

		/// Signer nomination
		type SignerNomination: SignerNomination<SignerId = Self::ValidatorId>;
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
		/// [broadcast_id, key_id, signatories, payload]
		ThresholdSignatureRequest(
			BroadcastId,
			T::KeyId,
			Vec<T::ValidatorId>,
			PayloadFor<T, I>,
		),
		/// [broadcast_id, validator_id, unsigned_tx]
		TransactionSigningRequest(
			BroadcastId,
			T::ValidatorId,
			UnsignedTransactionFor<T, I>,
		),
		/// [broadcast_id, signed_tx]
		ReadyForBroadcast(BroadcastId, SignedTransactionFor<T, I>),
		/// [broadcast_id]
		BroadcastComplete(BroadcastId),
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
		/// Submit a payload signature.
		///
		/// Triggers the next step in the signing process - requsting a transaction signature from a nominated validator.
		#[pallet::weight(10_000)]
		pub fn signature_ready(
			origin: OriginFor<T>,
			id: BroadcastId,
			signature: SignatureFor<T, I>,
		) -> DispatchResultWithPostInfo {
			let _ = T::EnsureWitnessed::ensure_origin(origin)?;

			// Construct the unsigned transaction and update the context.
			let unsigned_tx = PendingBroadcasts::<T, I>::try_mutate_exists(id, |maybe_ctx| {
				maybe_ctx
					.as_mut()
					.ok_or(Error::<T, I>::InvalidBroadcastId)
					.map(|ctx| ctx.construct_unsigned_transaction(&signature))
			})?;

			// Select a signer: we assume that the signature is a good source of randomness, so we use it to generate a
			// seed for the signer nomination.
			let seed: u64 = signature.using_encoded(|bytes| {
				let mut u64_bytes: [u8; 8] = [0xff; 8];
				let range_to_copy = 0..min(size_of::<u64>(), bytes.len());
				(&mut u64_bytes[range_to_copy.clone()]).copy_from_slice(&bytes[range_to_copy]);
				u64::from_be_bytes(u64_bytes)
			});
			let nominated_signer = T::SignerNomination::nomination_with_seed(seed);

			// Emit the transaction signing request.
			Self::deposit_event(Event::<T, I>::TransactionSigningRequest(
				id,
				nominated_signer,
				unsigned_tx,
			));

			Ok(().into())
		}

		///
		#[pallet::weight(10_000)]
		pub fn transaction_ready(
			origin: OriginFor<T>,
			id: BroadcastId,
			signed_tx: SignedTransactionFor<T, I>,
		) -> DispatchResultWithPostInfo {
			// TODO: - verify the signer is the same we requested the signature from.
			//       - can we also verify the actual signature?
			let signer = ensure_signed(origin)?;

			ensure!(
				PendingBroadcasts::<T, I>::contains_key(id),
				Error::<T, I>::InvalidBroadcastId
			);

			// Process the signed transaction callback and update the context.
			let _ = PendingBroadcasts::<T, I>::try_mutate_exists(id, |maybe_ctx| {
				maybe_ctx
					.as_mut()
					.ok_or(Error::<T, I>::InvalidBroadcastId)
					.map(|ctx| ctx.on_transaction_ready(&signed_tx))
			})?;

			// Q: Is this really necessary?
			Self::deposit_event(Event::<T, I>::ReadyForBroadcast(id, signed_tx));

			Ok(().into())
		}

		///
		#[pallet::weight(10_000)]
		pub fn broadcast_success(
			origin: OriginFor<T>,
			id: BroadcastId,
			tx_hash: TransactionHashFor<T, I>,
		) -> DispatchResultWithPostInfo {
			let _ = T::EnsureWitnessed::ensure_origin(origin)?;

			// Remove the broadcast, it's done.
			let _ = PendingBroadcasts::<T, I>::try_mutate_exists::<_, _, Error<T, I>, _>(id, |maybe_ctx| {
				let mut ctx = maybe_ctx
					.take()
					.ok_or(Error::<T, I>::InvalidBroadcastId)?;
				ctx.on_broadcast_success(&tx_hash);
				Ok(())
			})?;

			Self::deposit_event(Event::<T, I>::BroadcastComplete(id));

			Ok(().into())
		}

		///
		#[pallet::weight(10_000)]
		pub fn broadcast_failure(
			origin: OriginFor<T>,
			id: BroadcastId,
			failure: BroadcastFailure<<T as BaseConfig>::ValidatorId>,
		) -> DispatchResultWithPostInfo {
			let _ = T::EnsureWitnessed::ensure_origin(origin)?;

			match failure {
				BroadcastFailure::MpcFailure { bad_nodes: _ } => { todo!() },
				BroadcastFailure::TransactionFailure => todo!(),
				BroadcastFailure::TransactionTimeout => todo!(),
			}

			Ok(().into())
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	/// Initiates a broadcast and returns its id.
	pub fn initiate_broadcast(mut context: T::BroadcastContext, key_id: T::KeyId) -> u64 {
		// Get a new id.
		let id = BroadcastIdCounter::<T, I>::mutate(|id| {
			*id += 1;
			*id
		});

		let payload = context.construct_signing_payload();

		// Store the context.
		PendingBroadcasts::<T, I>::insert(id, &context);

		// Select nominees for threshold signature.
		// Q: does it matter if this is predictable? ie. does it matter if we use the `id`, which contains no randomness?
		let nominees = T::SignerNomination::threshold_nomination_with_seed(id);

		// Emit the initial request to the CFE.
		Self::deposit_event(Event::<T, I>::ThresholdSignatureRequest(
			id, key_id, nominees, payload,
		));

		id
	}
}
