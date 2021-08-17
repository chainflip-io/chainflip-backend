#![cfg_attr(not(feature = "std"), no_std)]
//! Request-Reply Pallet

use codec::{Encode, Decode};
use frame_support::dispatch::DispatchResult;
pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

trait ReqRep {
	type Reply;

	fn on_reply(&self, reply: Self::Reply) -> DispatchResult { todo!() }
}

// Requests

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
struct SignatureRequest<KeyId, ValidatorId> { signing_key: KeyId, payload: Vec<u8>, signatories: Vec<ValidatorId> }

impl<KeyId, ValidatorId> ReqRep for SignatureRequest<KeyId, ValidatorId> {
	type Reply = SignatureReply<ValidatorId>;
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
struct BroadcastRequest<Chain> { chain: Chain }

impl<Chain> ReqRep for BroadcastRequest<Chain> {
	type Reply = BroadcastReply;
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum Request<
	KeyId: Clone + PartialEq + Eq + Encode + Decode, 
	ValidatorId: Clone + PartialEq + Eq + Encode + Decode, 
	Chain: Clone + PartialEq + Eq + Encode + Decode
	> 
{
	SignatureRequest(SignatureRequest<KeyId, ValidatorId>),
	BroadcastRequest(BroadcastRequest<Chain>),
}

impl<
		KeyId: Clone + PartialEq + Eq + Encode + Decode,
		ValidatorId: Clone + PartialEq + Eq + Encode + Decode,
		Chain: Clone + PartialEq + Eq + Encode + Decode,
	> Request<KeyId, ValidatorId, Chain>
{
	fn upcast(self) -> Box<dyn ReqRep> {
		match self {
			Request::SignatureRequest(req) => Box::new(req),
			Request::BroadcastRequest(req) => Box::new(req),
		}
	}
}

/// Replies

enum SignatureReply<ValidatorId> {
	Success{ sig: Vec<u8> },
	Failure { bad_nodes: Vec<ValidatorId> },
}

struct BroadcastReply;

pub enum Reply<ValidatorId> {
	SignatureReply(SignatureReply<ValidatorId>),
	BroadcastReply(BroadcastReply),
}

impl<ValidatorId> Reply<ValidatorId> {
	fn try_downcast_reply<T>(self) {
		todo!()
	}
}

pub type RequestId = u64;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::dispatch::DispatchResultWithPostInfo;
	use frame_system::pallet_prelude::*;
	use frame_support::pallet_prelude::*;

	/// Configure the pallet by specifying the parameters and types on which it depends.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		type KeyId: Encode + Decode;

		/// The overarching request type.
		type Request: Encode + Decode;

		/// The overarching reply type.
		type Reply: Encode + Decode;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	#[pallet::getter(fn request_id_counter)]
	pub type RequestIdCounter<T> = StorageValue<_, RequestId, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn pending_request)]
	pub type PendingRequests<T> = StorageMap<_, RequestId, <T as Config>::Request, OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An outgoing request. [id, request]
		Request(RequestId, T::Request),
	}

	// Errors inform users that something went wrong.
	#[pallet::error]
	pub enum Error<T> {
		/// The provided request id is invalid.
		InvalidRequestId,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	// Dispatchable functions allows users to interact with the pallet and invoke state changes.
	// These functions materialize as "extrinsics", which are often compared to transactions.
	// Dispatchable functions must be annotated with a weight and must return a DispatchResult.
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Reply.
		#[pallet::weight(10_000)]
		pub fn reply(origin: OriginFor<T>, id: RequestId, reply: T::Reply) -> DispatchResultWithPostInfo {
			// Probably needs to be witnessed.
			let who = ensure_signed(origin)?;
			
			// 1. Pull the request type out of storage.
			let request = PendingRequests::<T>::get(id).ok_or(Error::<T>::InvalidRequestId)?;

			// 2. Figure out what the reply type is. (Try this? https://crates.io/crates/downcast-rs)
			// 3. Decode the reply.
			// 4. Dispatch the callback.
			let reply = request.upcast().on_reply(reply)?;

			Ok(().into())
		}
	}
}

impl<T: Config> Pallet<T> {
	pub fn request(request: impl ReqRep) {
		// Get a new id.
		let id = RequestIdCounter::<T>::mutate(|id| { *id += 1; *id });

		// Emit the request to the CFE.
		Self::deposit_event(Event::<T>::Request(id, Encode::encode(request)));
	}
}
