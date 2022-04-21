#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

use cf_traits::NetworkStateInfo;
use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;
pub use pallet::*;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weights;
pub use weights::WeightInfo;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum NetworkState {
	Paused,
	Running,
}

impl Default for NetworkState {
	fn default() -> Self {
		NetworkState::Running
	}
}
pub mod cfe {
	use super::*;
	/// On chain CFE settings
	#[derive(Encode, Decode, Clone, RuntimeDebug, PartialEq, Eq, Copy)]
	#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
	pub struct CfeSettings {
		/// Number of blocks we wait until we consider the ethereum witnesser stream finalized.
		pub eth_block_safety_margin: u32,
		/// Maximum duration a ceremony stage can last
		pub max_ceremony_stage_duration: u32,
	}

	/// Sensible default values for the CFE setting.
	impl Default for CfeSettings {
		fn default() -> Self {
			Self { eth_block_safety_margin: 6, max_ceremony_stage_duration: 300 }
		}
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	type EthereumAddress = [u8; 20];

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Because we want to emit events when there is a config change during
		/// an runtime upgrade
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		/// Governance origin to secure extrinsic
		type EnsureGovernance: EnsureOrigin<Self::Origin>;
		/// Weight information
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The network is currently paused.
		NetworkIsPaused,
	}

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::storage]
	#[pallet::getter(fn stake_manager_address)]
	/// The address of the ETH stake manager contract
	pub type StakeManagerAddress<T> = StorageValue<_, EthereumAddress, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn key_manager_address)]
	/// The address of the ETH key manager contract
	pub type KeyManagerAddress<T> = StorageValue<_, EthereumAddress, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn ethereum_chain_id)]
	/// The address of the ETH chain id
	pub type EthereumChainId<T> = StorageValue<_, u64, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn cfe_settings)]
	/// The settings used by the CFE
	pub type CfeSettings<T> = StorageValue<_, cfe::CfeSettings, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn is_network_paused)]
	/// Whether the network is paused
	pub type CurrentNetworkState<T> = StorageValue<_, NetworkState, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// The network state has been chagned \[state\]
		NetworkStateHasBeenChanged(NetworkState),
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Changes the current network state.
		///
		/// ## Events
		///
		/// - [NetworkStateHasBeenChanged](Event::NetworkStateHasBeenChanged)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_support::error::BadOrigin)
		#[pallet::weight(T::WeightInfo::set_network_state())]
		pub fn set_network_state(
			origin: OriginFor<T>,
			state: NetworkState,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;
			CurrentNetworkState::<T>::put(&state);
			Self::deposit_event(Event::NetworkStateHasBeenChanged(state));
			Ok(().into())
		}
	}

	#[pallet::genesis_config]
	#[cfg_attr(feature = "std", derive(Default))]
	pub struct GenesisConfig {
		pub stake_manager_address: EthereumAddress,
		pub key_manager_address: EthereumAddress,
		pub ethereum_chain_id: u64,
		pub cfe_settings: cfe::CfeSettings,
	}

	/// Sets the genesis config
	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig {
		fn build(&self) {
			StakeManagerAddress::<T>::set(self.stake_manager_address);
			KeyManagerAddress::<T>::set(self.key_manager_address);
			EthereumChainId::<T>::set(self.ethereum_chain_id);
			CfeSettings::<T>::set(self.cfe_settings);
		}
	}
}

pub struct NetworkStateAccess<T>(PhantomData<T>);

impl<T: Config> NetworkStateInfo for NetworkStateAccess<T> {
	fn ensure_paused() -> frame_support::sp_runtime::DispatchResult {
		if <pallet::CurrentNetworkState<T>>::get() == NetworkState::Paused {
			return Err(Error::<T>::NetworkIsPaused.into())
		}
		Ok(())
	}
}
