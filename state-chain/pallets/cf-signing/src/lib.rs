#![cfg_attr(not(feature = "std"), no_std)]
//! Request-Reply Pallet
#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

use codec::{Decode, Encode};

use frame_support::{Parameter, dispatch::{DispatchResultWithPostInfo, Dispatchable, PostDispatchInfo}, traits::UnfilteredDispatchable};
use frame_system::pallet_prelude::OriginFor;
pub use pallet::*;
use sp_runtime::RuntimeDebug;
use sp_std::marker::PhantomData;
use sp_std::prelude::*;
use cf_chains::Chain;
use cf_traits::{Chainflip, KeyProvider, SignerNomination, SigningContext};

pub type RequestId = u64;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use codec::FullCodec;
	use frame_support::pallet_prelude::*;
	use frame_support::{dispatch::DispatchResultWithPostInfo, Twox64Concat};
	use frame_system::{ensure_signed, pallet_prelude::*};
	use pallet_cf_reputation::{OfflineCondition, OfflineConditions};

	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode)]
	pub struct RequestContext<T: Chainflip, S: SigningContext<T> + Member> {
		pub signatories: Vec<T::ValidatorId>,
		pub chain_specific: S,
	}

	type SignatureFor<T, I> = <<T as Config<I>>::SigningContext as SigningContext<T>>::Signature;
	type PayloadFor<T, I> = <<T as Config<I>>::SigningContext as SigningContext<T>>::Payload;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config<I: 'static = ()>: Chainflip {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self, I>> + IsType<<Self as frame_system::Config>::Event>;

		/// A marker trait identifying the chain that we are signing for.
		type TargetChain: Chain;

		/// The context definition for this instance.
		type SigningContext: SigningContext<Self, Chain = Self::TargetChain> + Member + FullCodec;

		/// A type that allows us to check if a call was a result of witness consensus.
		type EnsureWitnessed: EnsureOrigin<Self::Origin>;

		/// Signer nomination.
		type SignerNomination: SignerNomination<SignerId = Self::ValidatorId>;

		/// Something that provides the current key for signing.
		type KeyProvider: KeyProvider<Self::TargetChain, KeyId = Self::KeyId>;

		/// For reporting bad actors.
		type OfflineConditions: OfflineConditions<ValidatorId = <Self as Chainflip>::ValidatorId>;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	#[pallet::storage]
	#[pallet::getter(fn request_id_counter)]
	pub type RequestIdCounter<T, I = ()> = StorageValue<_, RequestId, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn pending_request)]
	pub type PendingRequests<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, RequestId, RequestContext<T, T::SigningContext>, OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// [ceremony_id, key_id, signatories, payload]
		ThresholdSignatureRequest(RequestId, T::KeyId, Vec<T::ValidatorId>, PayloadFor<T, I>),
		/// [ceremony_id, key_id, offenders]
		ThresholdSignatureFailed(RequestId, T::KeyId, Vec<T::ValidatorId>),
		/// [ceremony_id]
		ThresholdSignatureSuccess(RequestId),
		/// [old_id, new_id]
		ThresholdSignatureRetry(RequestId, RequestId),
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// The provided request id is invalid.
		InvalidRequestId,
	}

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I> {}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Reply.
		#[pallet::weight(10_000)]
		pub fn signature_success(
			origin: OriginFor<T>,
			id: RequestId,
			signature: SignatureFor<T, I>,
		) -> DispatchResultWithPostInfo {
			let _ = T::EnsureWitnessed::ensure_origin(origin.clone())?;

			// 1. Ensure the id is valid and remove the context.
			let context =
				PendingRequests::<T, I>::take(id).ok_or(Error::<T, I>::InvalidRequestId)?;
			
			// TODO: verify the threshold signature.

			Self::deposit_event(Event::<T, I>::ThresholdSignatureSuccess(id));

			// 2. Dispatch the callback.
			context.chain_specific.dispatch_callback(origin, signature)
		}

		/// Reply.
		#[pallet::weight(10_000)]
		pub fn signature_failed(
			origin: OriginFor<T>,
			id: RequestId,
			offenders: Vec<<T as Chainflip>::ValidatorId>,
		) -> DispatchResultWithPostInfo {
			const PENALTY: i32 = 15; // TODO: This should probably be specified somewhere common for all penalties.
			let _ = T::EnsureWitnessed::ensure_origin(origin.clone())?;

			// Report the offenders.
			for offender in offenders.iter() {
				T::OfflineConditions::report(OfflineCondition::ParticipateSigningFailed, PENALTY, offender)
					.unwrap_or_else(|e| {
						frame_support::debug::error!("Unable to report offense for signer {:?}: {:?}", offender, e);
						0
					});
			}

			// Remove the context and retry.
			let context =
				PendingRequests::<T, I>::take(id).ok_or(Error::<T, I>::InvalidRequestId)?;

			let retry_id = Self::request_signature(context.chain_specific);

			Self::deposit_event(Event::<T, I>::ThresholdSignatureRetry(id, retry_id));

			Ok(().into())
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	/// Emits a request event, stores it, and returns its id.
	pub fn request_signature(context: T::SigningContext) -> u64 {
		// Get a new id.
		let id = RequestIdCounter::<T, I>::mutate(|id| {
			*id += 1;
			*id
		});

		// Get the current signing key.
		let key_id = T::KeyProvider::current_key();

		// Construct the payload.
		let payload = context.get_payload();

		// Select nominees for threshold signature.
		// Q: does it matter if this is predictable? ie. does it matter if we use the `id` as a seed value?
		let nominees = T::SignerNomination::threshold_nomination_with_seed(id);
		
		// Store the context.
		PendingRequests::<T, I>::insert(id, RequestContext {
			signatories: nominees.clone(),
			chain_specific: context,
		});

		// Emit the request to the CFE.
		Self::deposit_event(Event::<T, I>::ThresholdSignatureRequest(
			id, key_id, nominees, payload,
		));

		id
	}
}

impl<T, I: 'static> cf_traits::ThresholdSigner<T> for Pallet<T, I>
where
	T: Config<I>,
{
	type Context = T::SigningContext;

	fn request_signature(context: Self::Context) -> u64 {
		Self::request_signature(context)
	}
}
