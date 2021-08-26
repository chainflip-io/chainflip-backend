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

use frame_support::Parameter;
use frame_system::pallet_prelude::OriginFor;
pub use pallet::*;
use sp_std::prelude::*;
use sp_std::{cmp::min, marker::PhantomData, mem::size_of};

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

pub enum BroadcastOutcome<Receipt: Parameter> {
	Success(Receipt),
	Failure,
	Timeout,
}

pub trait BroadcastContext<ChainId> {
	const CHAIN_ID: ChainId;

	type Payload: Parameter;
	type Signature: Parameter;
	type UnsignedTransaction: Parameter;
	type SignedTransaction: Parameter;

	/// Constructs the payload for the threshold signature.
	fn construct_signing_payload(&mut self) -> Self::Payload;

	/// Constructs the outgoing transaction using the payload signature.
	fn construct_unsigned_transaction(
		&mut self,
		sig: &Self::Signature,
	) -> Self::UnsignedTransaction;

	/// Callback for when the signed transaction is submitted to the state chain.
	fn on_transaction_ready(&mut self, signed_tx: &Self::SignedTransaction);
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
		struct EthBroadcaster;

		impl BroadcastContext<ChainId> for EthBroadcaster {
			const CHAIN_ID: ChainId = ChainId::Eth;

			type Payload = H256;
			type Signature = H256;
			type UnsignedTransaction = ();
			type SignedTransaction = ();

			fn construct_signing_payload(&mut self) -> Self::Payload {
				todo!()
			}

			fn construct_unsigned_transaction(
				&mut self,
				sig: &Self::Signature,
			) -> Self::UnsignedTransaction {
				todo!()
			}

			fn on_transaction_ready(&mut self, signed_tx: &Self::SignedTransaction) {
				todo!()
			}
		}
	}
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

	pub type PayloadFor<T, I> = <<T as Config<I>>::BroadcastContext as BroadcastContext<
		<T as BaseConfig>::ChainId,
	>>::Payload;
	pub type SignatureFor<T, I> = <<T as Config<I>>::BroadcastContext as BroadcastContext<
		<T as BaseConfig>::ChainId,
	>>::Signature;
	pub type SignedTransactionFor<T, I> = <<T as Config<I>>::BroadcastContext as BroadcastContext<<T as BaseConfig>::ChainId>>::SignedTransaction;
	pub type UnsignedTransactionFor<T, I> = <<T as Config<I>>::BroadcastContext as BroadcastContext<<T as BaseConfig>::ChainId>>::UnsignedTransaction;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config<I: 'static = ()>: BaseConfig {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self, I>> + IsType<<Self as frame_system::Config>::Event>;

		/// A type that allows us to check if a call was a result of witness consensus.
		type EnsureWitnessed: EnsureOrigin<Self::Origin>;

		/// The context definition for this instance.
		type BroadcastContext: BroadcastContext<Self::ChainId> + Member + FullCodec;

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
		///
		ThresholdSignatureRequest(
			BroadcastId,
			Vec<<T as BaseConfig>::ValidatorId>,
			PayloadFor<T, I>,
		),
		///
		TransactionSigningRequest(
			BroadcastId,
			<T as BaseConfig>::ValidatorId,
			UnsignedTransactionFor<T, I>,
		),
		///
		ReadyForBroadcast(BroadcastId, SignedTransactionFor<T, I>),
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
			let _ = T::EnsureWitnessed::ensure_origin(origin.clone())?;

			// Construct the unsigned transaction and update the context.
			let unsigned_tx = PendingBroadcasts::<T, I>::try_mutate_exists(id, |maybe_ctx| {
				maybe_ctx
					.as_mut()
					.map(|ctx| ctx.construct_unsigned_transaction(&signature))
					.ok_or(Error::<T, I>::InvalidBroadcastId)
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
			let signer = ensure_signed(origin.clone())?;

			ensure!(
				PendingBroadcasts::<T, I>::contains_key(id),
				Error::<T, I>::InvalidBroadcastId
			);

			// Construct the unsigned transaction and update the context.
			let _ = PendingBroadcasts::<T, I>::try_mutate_exists(id, |maybe_ctx| {
				maybe_ctx
					.as_mut()
					.map(|ctx| ctx.on_transaction_ready(&signed_tx))
					.ok_or(Error::<T, I>::InvalidBroadcastId)
			})?;

			// Q: Is this really necessary?
			Self::deposit_event(Event::<T, I>::ReadyForBroadcast(id, signed_tx));

			Ok(().into())
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	/// Initiates a broadcast and returns its id.
	pub fn initiate_broadcast(mut context: T::BroadcastContext) -> u64 {
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
			id, nominees, payload,
		));

		id
	}
}
