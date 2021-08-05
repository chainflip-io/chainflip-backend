#![cfg_attr(not(feature = "std"), no_std)]
pub use pallet::*;
use sp_runtime::DispatchError;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;
	use sp_core::storage::well_known_keys;
	use sp_std::vec::Vec;

	type AccountId<T> = <T as frame_system::Config>::AccountId;
	#[pallet::config]
	pub trait Config: frame_system::Config {
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
	}
	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	#[pallet::getter(fn code)]
	pub type Code<T> = StorageValue<_, Vec<u8>>;

	#[pallet::storage]
	#[pallet::getter(fn voted)]
	pub type Voted<T> = StorageValue<_, Vec<AccountId<T>>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn members)]
	pub type Members<T> = StorageValue<_, Vec<AccountId<T>>>;

	#[pallet::storage]
	#[pallet::getter(fn votes)]
	pub type Votes<T> = StorageValue<_, u32>;

	#[pallet::storage]
	#[pallet::getter(fn required_approvals)]
	pub type RequiredApprovals<T> = StorageValue<_, u32>;

	#[pallet::storage]
	#[pallet::getter(fn expiry_block)]
	pub type ExpiryBlock<T> = StorageValue<_, u32>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			match (<Votes<T>>::get(), <Code<T>>::get()) {
				(Some(votes), Some(code)) if Self::majority_reached(votes) => {
					storage::unhashed::put_raw(well_known_keys::CODE, &code);
					Self::cleanup();
					Self::deposit_event(Event::RuntimeUpdated);
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
		ProposedRuntimeUpgrade(Vec<u8>, T::AccountId),
		RuntimeUpdated,
		Voted,
	}

	#[pallet::error]
	pub enum Error<T> {
		AlreadyVoted,
		OnGoingUpgrade,
		NoMember,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(10_000)]
		pub fn propose_runtime_upgrade(
			origin: OriginFor<T>,
			code: Vec<u8>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			Self::ensure_member(&who)?;
			ensure!(<Code<T>>::get().is_none(), Error::<T>::OnGoingUpgrade);
			<Code<T>>::put(code.clone());
			Self::deposit_event(Event::ProposedRuntimeUpgrade(code.clone(), who));
			Ok(().into())
		}

		#[pallet::weight(10_000)]
		pub fn approve_runtime_upgrade(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
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
				members: vec![],
				required_approvals: 0,
			}
		}
	}

	// The build of genesis for the pallet.
	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			Members::<T>::put(self.members.clone());
			RequiredApprovals::<T>::put(self.required_approvals);
		}
	}
}

impl<T: Config> Pallet<T> {
	fn majority_reached(votes: u32) -> bool {
		votes > <RequiredApprovals<T>>::get().unwrap()
	}
	fn ensure_member(account: &T::AccountId) -> Result<(), DispatchError> {
		match <Members<T>>::get() {
			Some(members) if members.contains(account) => Ok(()),
			_ => Err(Error::<T>::NoMember.into()),
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
		1000000000
	}
	fn vote(account: T::AccountId) -> Result<(), DispatchError> {
		if let Some(votes) = <Votes<T>>::get() {
			<Votes<T>>::put(votes + 1);
		} else {
			<Votes<T>>::put(1);
		}
		Self::deposit_event(Event::Voted);
		<Voted<T>>::mutate(|votes| votes.push(account));
		Ok(())
	}
	fn cleanup() {
		<Code<T>>::take();
		<Votes<T>>::take();
	}
}
