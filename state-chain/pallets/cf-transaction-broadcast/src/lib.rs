#![cfg_attr(not(feature = "std"), no_std)]
// This can be removed after rustc version 1.53.
#![feature(int_bits_const)]

//! Transaction Broadcast Pallet
//! https://swimlanes.io/d/DJNaWp1Go

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod instances;

use cf_traits::NonceProvider;
use codec::{Decode, Encode};
use frame_support::Parameter;
use frame_system::pallet_prelude::OriginFor;
pub use pallet::*;
use sp_std::prelude::*;
use sp_std::{cmp::min, marker::PhantomData, mem::size_of};

pub enum ChainId {
	Ethereum,
}

pub trait BaseConfig: frame_system::Config {
	/// The id type used to identify individual signing keys.
	type KeyId: Parameter;
	type ValidatorId: Parameter
		+ Into<<Self as frame_system::Config>::AccountId>
		+ From<<Self as frame_system::Config>::AccountId>;
	type ChainId: Parameter;
	type NonceProvider: NonceProvider;
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum BroadcastFailure<SignerId: Parameter> {
	/// The threshold signature step failed, we need to blacklist some nodes.
	MpcFailure { bad_nodes: Vec<SignerId> },
	/// The nominated signer was unable to sign the transaction in time.
	SigningTimeout(SignerId),
	/// The transaction was rejected because of some user error.
	TransactionRejected,
	/// The transaction failed for some unknown reason.
	TransactionFailed,
	/// The transaction stalled.
	TransactionTimeout,
}

pub trait BroadcastContext<T: BaseConfig> {
	/// The payload type that will be signed over.
	type Payload: Parameter;
	/// The signature type that is returned by the threshold signature.
	type Signature: Parameter;
	/// An unsigned version of the transaction that needs to be broadcast.
	type UnsignedTransaction: Parameter;
	type SignedTransaction: Parameter;
	type TransactionHash: Parameter;
	type Error;

	/// Constructs the payload for the threshold signature.
	fn construct_signing_payload(&self) -> Result<Self::Payload, Self::Error>;

	/// Adds the signature to the broadcast context.
	fn add_threshold_signature(&mut self, sig: &Self::Signature);

	/// Constructs the outgoing transaction.
	fn construct_unsigned_transaction(&self) -> Result<Self::UnsignedTransaction, Self::Error>;

	/// Verify the signed transaction when it is submitted to the state chain.
	fn verify_tx(
		&self,
		signer: &T::ValidatorId,
		signed_tx: &Self::SignedTransaction,
	) -> Result<(), Self::Error>;
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

pub trait KeyProvider<Chain: Into<ChainId>> {
	type KeyId;

	fn current_key() -> Self::KeyId;
}

pub type BroadcastId = u64;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use codec::FullCodec;
	use frame_support::pallet_prelude::*;
	use frame_support::{dispatch::DispatchResultWithPostInfo, Twox64Concat};
	use frame_system::pallet_prelude::*;

	pub type PayloadFor<T, I> =
		<<T as Config<I>>::BroadcastContext as BroadcastContext<T>>::Payload;
	pub type SignatureFor<T, I> =
		<<T as Config<I>>::BroadcastContext as BroadcastContext<T>>::Signature;
	pub type SignedTransactionFor<T, I> =
		<<T as Config<I>>::BroadcastContext as BroadcastContext<T>>::SignedTransaction;
	pub type UnsignedTransactionFor<T, I> =
		<<T as Config<I>>::BroadcastContext as BroadcastContext<T>>::UnsignedTransaction;
	pub type TransactionHashFor<T, I> =
		<<T as Config<I>>::BroadcastContext as BroadcastContext<T>>::TransactionHash;
	pub type BroadcastErrorFor<T, I> =
		<<T as Config<I>>::BroadcastContext as BroadcastContext<T>>::Error;
	pub type BroadcastFailureFor<T> = BroadcastFailure<T>;

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
	pub enum BroadcastState {
		AwaitingThreshold,
		AwaitingSignature,
		AwaitingBroadcast,
		Complete,
		Failed,
	}

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config<I: 'static = ()>: BaseConfig {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self, I>> + IsType<<Self as frame_system::Config>::Event>;

		/// A marker trait identifying the chain that is broadcast to.
		type TargetChain: Into<ChainId>;

		/// A type that allows us to check if a call was a result of witness consensus.
		type EnsureWitnessed: EnsureOrigin<Self::Origin>;

		/// The context definition for this instance.
		type BroadcastContext: BroadcastContext<Self> + Member + FullCodec;

		/// Signer nomination.
		type SignerNomination: SignerNomination<SignerId = Self::ValidatorId>;

		/// Something that provides the current key for signing.
		type KeyProvider: KeyProvider<Self::TargetChain, KeyId = Self::KeyId>;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	#[pallet::storage]
	#[pallet::getter(fn request_id_counter)]
	pub type BroadcastIdCounter<T, I = ()> = StorageValue<_, BroadcastId, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn pending_request)]
	pub type PendingBroadcasts<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
		_,
		Identity,
		BroadcastState,
		Twox64Concat,
		BroadcastId,
		T::BroadcastContext,
		OptionQuery,
	>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// [broadcast_id, key_id, signatories, payload]
		ThresholdSignatureRequest(BroadcastId, T::KeyId, Vec<T::ValidatorId>, PayloadFor<T, I>),
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
		/// The provided request id is invalid.
		InvalidSignature,
		/// The outgoing transaction could not be constructed.
		TransactionConstructionFailed,
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

			let mut context =
				PendingBroadcasts::<T, I>::take(BroadcastState::AwaitingThreshold, id)
					.ok_or(Error::<T, I>::InvalidBroadcastId)?;

			// Construct the unsigned transaction and update the context.
			context.add_threshold_signature(&signature);
			let unsigned_tx = context.construct_unsigned_transaction().map_err(|_| {
				// We should only reach here if he have invalid data. If this is the case, restarting
				// won't help. The broacast has failed.
				PendingBroadcasts::<T, I>::insert(BroadcastState::Failed, id, context.clone());
				Error::<T, I>::TransactionConstructionFailed
			})?;

			PendingBroadcasts::<T, I>::insert(BroadcastState::AwaitingSignature, id, context);

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

		/// Called when the transaction is ready to be broadcast. The signed transaction is stored on-chain so that
		/// any node can potentially broadcast it to the target chain.
		#[pallet::weight(10_000)]
		pub fn transaction_ready(
			origin: OriginFor<T>,
			id: BroadcastId,
			signed_tx: SignedTransactionFor<T, I>,
		) -> DispatchResultWithPostInfo {
			let signer = ensure_signed(origin)?;

			// Process the signed transaction callback and update the context.
			let context = PendingBroadcasts::<T, I>::take(BroadcastState::AwaitingSignature, id)
				.ok_or(Error::<T, I>::InvalidBroadcastId)?;

			context
				.verify_tx(&signer.into(), &signed_tx)
				.unwrap_or_else(|_| {
					todo!()
					// the authored transaction is invalid.
					// punish the signer and nominate a new one.
				});

			// Q: Is this really necessary? We could also listen to the `AwaitingBroadcast` storage location.
			Self::deposit_event(Event::<T, I>::BroadcastRequest(id, signed_tx));

			PendingBroadcasts::<T, I>::insert(BroadcastState::AwaitingBroadcast, id, context);

			Ok(().into())
		}

		/// Nodes have witnessed that the transaction has reached finality on the target chain.
		#[pallet::weight(10_000)]
		pub fn broadcast_success(
			origin: OriginFor<T>,
			id: BroadcastId,
			tx_hash: TransactionHashFor<T, I>,
		) -> DispatchResultWithPostInfo {
			let _ = T::EnsureWitnessed::ensure_origin(origin)?;

			// Move it to the Complete storage area.
			let mut context =
				PendingBroadcasts::<T, I>::take(BroadcastState::AwaitingBroadcast, id)
					.ok_or(Error::<T, I>::InvalidBroadcastId)?;

			PendingBroadcasts::<T, I>::insert(BroadcastState::Complete, id, context);

			Self::deposit_event(Event::<T, I>::BroadcastComplete(id));

			Ok(().into())
		}

		/// Nodes have witnessed that something went wrong. Either the threshold signature failed, or the transaction
		/// was rejected or stalled on the target chain.
		#[pallet::weight(10_000)]
		pub fn broadcast_failure(
			origin: OriginFor<T>,
			id: BroadcastId,
			failure: BroadcastFailure<<T as BaseConfig>::ValidatorId>,
		) -> DispatchResultWithPostInfo {
			let _ = T::EnsureWitnessed::ensure_origin(origin)?;

			match failure {
				BroadcastFailure::MpcFailure { bad_nodes: _ } => {
					// Report bad nodes and schedule for retry
					todo!()
				}
				BroadcastFailure::SigningTimeout(signer) => {
					// Report and nominate a new signer. Retry.
					todo!()
				}
				BroadcastFailure::TransactionRejected => {
					// Report and nominate a new signer. Retry.
					todo!()
				}
				BroadcastFailure::TransactionFailed => {
					// This is bad.
					todo!()
				}
				BroadcastFailure::TransactionTimeout => {
					// Nominate a new signer, but don't report the old one. Retry.
					todo!()
				}
			}

			Ok(().into())
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	/// Initiates a broadcast and returns its id.
	pub fn initiate_broadcast(
		context: T::BroadcastContext,
	) -> Result<BroadcastId, BroadcastErrorFor<T, I>> {
		// Get a new id.
		let id = BroadcastIdCounter::<T, I>::mutate(|id| {
			*id += 1;
			*id
		});

		// Get the current signing key.
		let key_id = T::KeyProvider::current_key();

		// Construct the payload.
		let payload = context.construct_signing_payload()?;

		// Store the context.
		PendingBroadcasts::<T, I>::insert(BroadcastState::AwaitingThreshold, id, &context);

		// Select nominees for threshold signature.
		// Q: does it matter if this is predictable? ie. does it matter if we use the `id`, which contains no randomness?
		let nominees = T::SignerNomination::threshold_nomination_with_seed(id);

		// Emit the initial request to the CFE.
		Self::deposit_event(Event::<T, I>::ThresholdSignatureRequest(
			id, key_id, nominees, payload,
		));

		Ok(id)
	}
}
