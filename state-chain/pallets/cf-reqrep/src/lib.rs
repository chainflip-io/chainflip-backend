#![cfg_attr(not(feature = "std"), no_std)]
//! Request-Reply Pallet
#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

use frame_support::{Parameter, dispatch::DispatchResult};
pub use pallet::*;

pub trait ReqRep<T> : Parameter {
	type Reply: Parameter;

	fn on_reply(&self, _reply: Self::Reply) -> DispatchResult { todo!() }
}

pub mod reqreps {
	pub use super::*;
	use codec::{Decode, Encode};
	use frame_support::Parameter;

	pub trait BaseConfig: frame_system::Config {
		/// The id type used to identify signing keys.
		type KeyId: Parameter;
		type ValidatorId: Parameter;
		type ChainId: Parameter;
	}

	pub mod signature {
		use super::*;

		#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
		pub struct Request<KeyId, ValidatorId> {
			signing_key: KeyId,
			payload: Vec<u8>,
			signatories: Vec<ValidatorId>,
		}

		#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
		pub enum Reply<ValidatorId> {
			Success { sig: Vec<u8> },
			Failure { bad_nodes: Vec<ValidatorId> },
		}

		impl<T: BaseConfig> ReqRep<T> for Request<T::KeyId, T::ValidatorId> {
			type Reply = Reply<T::ValidatorId>;
		}
	}

	pub mod broadcast {
		use super::*;

		#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
		pub struct Request<ChainId> {
			chain: ChainId,
		}

		#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
		pub enum Reply {
			Success,
			Failure,
			Timeout,
		}

		impl<T: BaseConfig> ReqRep<T> for Request<T::ChainId> {
			type Reply = Reply;
		}
	}
}

pub type RequestId = u64;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use std::marker::PhantomData;
	use frame_support::{Twox64Concat, dispatch::DispatchResultWithPostInfo};
	use frame_system::pallet_prelude::*;
	use frame_support::pallet_prelude::*;

	type ReplyFor<T, I> = <<T as Config<I>>::Request as ReqRep<T>>::Reply;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config<I: 'static = ()>: reqreps::BaseConfig {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self, I>> + IsType<<Self as frame_system::Config>::Event>;

		/// The request-reply definition for this instance.
		type Request: ReqRep<Self>;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	#[pallet::storage]
	#[pallet::getter(fn request_id_counter)]
	pub type RequestIdCounter<T, I = ()> = StorageValue<_, RequestId, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn pending_request)]
	pub type PendingRequests<T: Config<I>, I: 'static = ()> = StorageMap<_, Twox64Concat, RequestId, T::Request, OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// An outgoing request. [id, request]
		Request(RequestId, T::Request),
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// The provided request id is invalid.
		InvalidRequestId,
	}

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I> {}

	// Dispatchable functions allows users to interact with the pallet and invoke state changes.
	// These functions materialize as "extrinsics", which are often compared to transactions.
	// Dispatchable functions must be annotated with a weight and must return a DispatchResult.
	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Reply.
		#[pallet::weight(10_000)]
		pub fn reply(origin: OriginFor<T>, id: RequestId, reply: ReplyFor<T, I>) -> DispatchResultWithPostInfo {
			// Probably needs to be witnessed.
			let _who = ensure_signed(origin)?;
			
			// 1. Pull the request type out of storage.
			let request = PendingRequests::<T, I>::get(id).ok_or(Error::<T, I>::InvalidRequestId)?;

			// 2. Dispatch the callback.
			let _ = request.on_reply(reply)?;

			Ok(().into())
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	pub fn request(request: T::Request) {
		// Get a new id.
		let id = RequestIdCounter::<T, I>::mutate(|id| { *id += 1; *id });

		// Emit the request to the CFE.
		Self::deposit_event(Event::<T, I>::Request(id, request));
	}
}
