#![cfg_attr(not(feature = "std"), no_std)]
use codec::Decode;
pub use pallet::*;
use sp_runtime::DispatchError;
use sp_std::vec::Vec;
#[frame_support::pallet]
pub mod pallet {
	use frame_support::{
		dispatch::{GetDispatchInfo, PostDispatchInfo},
		pallet_prelude::*,
		traits::UnixTime,
	};

	use codec::Encode;
	use frame_system::pallet_prelude::*;
	use sp_runtime::traits::Dispatchable;
	use sp_std::boxed::Box;
	use sp_std::vec::Vec;

	type AccountId<T> = <T as frame_system::Config>::AccountId;
	type OpaqueCall = Vec<u8>;
	#[pallet::config]
	pub trait Config: frame_system::Config {
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		type Call: Parameter
			+ Dispatchable<Origin = Self::Origin, PostInfo = PostDispatchInfo>
			+ GetDispatchInfo
			+ From<frame_system::Call<Self>>;
		type TimeSource: UnixTime;
	}
	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	#[pallet::getter(fn call)]
	pub type SudoCall<T> = StorageValue<_, OpaqueCall>;

	#[pallet::storage]
	#[pallet::getter(fn voted)]
	pub type Voted<T> = StorageValue<_, Vec<AccountId<T>>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn members)]
	pub(super) type Members<T> = StorageValue<_, Vec<AccountId<T>>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn votes)]
	pub type Votes<T> = StorageValue<_, u32>;

	#[pallet::storage]
	#[pallet::getter(fn required_approvals)]
	pub(super) type RequiredApprovals<T> = StorageValue<_, u32>;

	#[pallet::storage]
	#[pallet::getter(fn expiry_block)]
	pub type ExpiryTime<T> = StorageValue<_, u64>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			if let Some(expiry_time) = <ExpiryTime<T>>::get() {
				if T::TimeSource::now().as_secs() >= expiry_time {
					Self::deposit_event(Event::SudoCallExpired);
					Self::cleanup();
				}
			}
			match (<Votes<T>>::get(), <SudoCall<T>>::get()) {
				(Some(votes), Some(encoded_call)) if Self::majority_reached(votes) => {
					if let Some(call) = Self::decode_call(encoded_call) {
						let result = call.dispatch(frame_system::RawOrigin::Root.into());
						if result.is_ok() {
							Self::deposit_event(Event::CallExecuted);
							Self::cleanup();
						}
					}
					Self::calc_block_weight()
				}
				_ => 0,
			}
		}
	}

	#[pallet::event]
	#[pallet::metadata(T::AccountId = "AccountId")]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		ProposedSudoCall(Vec<u8>, T::AccountId),
		CallExecuted,
		Voted,
		SudoCallExpired,
	}

	#[pallet::error]
	pub enum Error<T> {
		AlreadyVoted,
		OnGoingVote,
		NoMember,
		CanNotDecodeCall,
		CanNotExecuteCall,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(10_000)]
		pub fn propose_sudo_call(
			origin: OriginFor<T>,
			call: Box<<T as Config>::Call>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			ensure!(<SudoCall<T>>::get().is_none(), Error::<T>::OnGoingVote);
			Self::ensure_member(&who)?;
			// 3 minutes
			<ExpiryTime<T>>::put(T::TimeSource::now().as_secs() + 180);
			<SudoCall<T>>::put(call.encode());
			Self::deposit_event(Event::ProposedSudoCall(call.encode(), who.clone()));
			Ok(().into())
		}

		#[pallet::weight(10_000)]
		pub fn approve_sudo_call(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			Self::ensure_member(&who)?;
			Self::ensure_not_voted(&who)?;
			Self::vote(who.clone())?;
			Ok(().into())
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub members: Vec<AccountId<T>>,
		pub required_approvals: u32,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				members: Default::default(),
				required_approvals: Default::default(),
			}
		}
	}

	// The build of genesis for the pallet.
	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			Members::<T>::set(self.members.clone());
			RequiredApprovals::<T>::set(Some(self.required_approvals));
		}
	}
}

impl<T: Config> Pallet<T> {
	fn majority_reached(votes: u32) -> bool {
		votes >= <RequiredApprovals<T>>::get().unwrap()
	}
	fn ensure_member(account: &T::AccountId) -> Result<(), DispatchError> {
		if !<Members<T>>::get().contains(account) {
			Err(Error::<T>::NoMember.into())
		} else {
			Ok(())
		}
	}
	fn ensure_not_voted(account: &T::AccountId) -> Result<(), DispatchError> {
		if <Voted<T>>::get().contains(account) {
			Err(Error::<T>::AlreadyVoted.into())
		} else {
			Ok(())
		}
	}
	fn calc_block_weight() -> u64 {
		// TODO: figure out what makes sense here
		0
	}
	fn vote(account: T::AccountId) -> Result<(), DispatchError> {
		match <Votes<T>>::get() {
			Some(votes) => {
				<Votes<T>>::put(votes + 1);
			}
			None => {
				<Votes<T>>::put(1);
			}
		}
		Self::deposit_event(Event::Voted);
		<Voted<T>>::mutate(|votes| votes.push(account));
		Ok(())
	}
	fn decode_call(call: Vec<u8>) -> Option<<T as Config>::Call> {
		Decode::decode(&mut &call[..]).ok()
	}
	fn cleanup() {
		<Votes<T>>::take();
		<Voted<T>>::take();
		<SudoCall<T>>::take();
		<ExpiryTime<T>>::take();
	}
}
