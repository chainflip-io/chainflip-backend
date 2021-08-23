#![cfg_attr(not(feature = "std"), no_std)]
//! Request-Reply Pallet
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

pub trait RequestContext<T: frame_system::Config> {
	type Response: Parameter;
	type Callback: Dispatchable<PostInfo = PostDispatchInfo, Origin = T::Origin>
		+ codec::Codec
		+ Clone
		+ PartialEq
		+ Eq;

	fn get_callback(&self, response: Self::Response) -> Self::Callback;

	fn dispatch_callback(
		&self,
		origin: <Self::Callback as Dispatchable>::Origin,
		response: Self::Response,
	) -> DispatchResultWithPostInfo {
		self.get_callback(response).dispatch(origin).into()
	}
}

#[derive(Clone, Copy, PartialEq, Eq, Encode, Decode)]
struct NullCallback<T>(PhantomData<T>);

impl<T: frame_system::Config> Dispatchable for NullCallback<T> {
	type Origin = T::Origin;
	type Config = T;
	type Info = ();
	type PostInfo = PostDispatchInfo;

	fn dispatch(self, origin: Self::Origin) -> sp_runtime::DispatchResultWithInfo<Self::PostInfo> {
		Ok(().into())
	}
}

pub trait BaseConfig: frame_system::Config + std::fmt::Debug {
	/// The id type used to identify individual signing keys.
	type KeyId: Parameter;
	type ValidatorId: Parameter;
	type ChainId: Parameter;
}

// These would be defined in their own modules but adding it here for now.
// Macros might help reduce the boilerplat but I don't think it's too bad.
pub mod instances {
	pub use super::*;
	use codec::{Decode, Encode};

	// A signature request.
	pub mod signing {
		use super::*;
		use sp_std::marker::PhantomData;

		#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode)]
		pub struct Request<T: BaseConfig> {
			signing_key: T::KeyId,
			payload: Vec<u8>,
			signatories: Vec<T::ValidatorId>,
		}

		#[derive(Clone, PartialEq, Eq, Encode, Decode)]
		pub enum Response<T: BaseConfig> {
			Success { sig: Vec<u8> },
			Failure { bad_nodes: Vec<T::ValidatorId> },
		}

		impl<T: BaseConfig> sp_std::fmt::Debug for Response<T> {
			fn fmt(&self, f: &mut sp_std::fmt::Formatter<'_>) -> sp_std::fmt::Result {
				f.write_str(stringify!(Response))
			}
		}

		struct SigningRequestResponse<T>(PhantomData<T>);

		impl<T: BaseConfig> RequestContext<T> for SigningRequestResponse<T> {
			type Response = Response<T>;
			type Callback = NullCallback<T>;

			fn get_callback(&self, response: Self::Response) -> Self::Callback {
				todo!("Delegate to some call.")
			}
		}
	}

	// A broadcast request.
	pub mod broadcast {
		use super::*;
		use sp_std::marker::PhantomData;

		#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
		pub struct Request<T: BaseConfig> {
			chain: T::ChainId,
		}

		#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
		pub enum Response {
			Success,
			Failure,
			Timeout,
		}

		struct BroadcastRequestResponse<T>(PhantomData<T>);

		impl<T: BaseConfig> RequestContext<T> for BroadcastRequestResponse<T> {
			type Response = Response;
			type Callback = NullCallback<T>;

			fn get_callback(&self, response: Self::Response) -> Self::Callback {
				todo!("Delegate to some call.")
			}
		}
	}
}

pub type RequestId = u64;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use codec::FullCodec;
	use frame_support::pallet_prelude::*;
	use frame_support::{dispatch::DispatchResultWithPostInfo, Twox64Concat};
	use frame_system::{ensure_signed, pallet_prelude::*};

	type ResponseFor<T, I> = <<T as Config<I>>::RequestContext as RequestContext<T>>::Response;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config<I: 'static = ()>: BaseConfig {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self, I>> + IsType<<Self as frame_system::Config>::Event>;

		/// The request-response definition for this instance.
		type RequestContext: RequestContext<Self> + Member + FullCodec;
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
		StorageMap<_, Twox64Concat, RequestId, T::RequestContext, OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// An outgoing request. [id, request]
		Request(RequestId, T::RequestContext),
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
		pub fn response(
			origin: OriginFor<T>,
			id: RequestId,
			response: ResponseFor<T, I>,
		) -> DispatchResultWithPostInfo {
			let _ = ensure_signed(origin.clone())?;

			// 1. Pull the context out of storage.
			let context =
				PendingRequests::<T, I>::take(id).ok_or(Error::<T, I>::InvalidRequestId)?;

			// 2. Dispatch the callback.
			context.dispatch_callback(origin, response)
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	/// Emits a request event, stores it, and returns its id.
	pub fn request(request: T::RequestContext) -> u64 {
		// Get a new id.
		let id = RequestIdCounter::<T, I>::mutate(|id| {
			*id += 1;
			*id
		});

		// Store the request.
		PendingRequests::<T, I>::insert(id, &request);

		// Emit the request to the CFE.
		Self::deposit_event(Event::<T, I>::Request(id, request));

		id
	}
}
