#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;
#[frame_support::pallet]
pub mod pallet {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	type ConfigItem = Vec<u8>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	#[pallet::getter(fn stake_manager_address)]
	pub type StakeManagerAddress<T> = StorageValue<_, ConfigItem>;

	#[pallet::storage]
	#[pallet::getter(fn key_manager_address)]
	pub type KeyManagerAddress<T> = StorageValue<_, ConfigItem>;

	#[pallet::storage]
	#[pallet::getter(fn ethereum_chain_id)]
	pub type EthereumChainId<T> = StorageValue<_, ConfigItem>;

	#[pallet::storage]
	#[pallet::getter(fn ethereum_vault_address)]
	pub type EthereumVaultAddress<T> = StorageValue<_, ConfigItem>;

	#[pallet::event]
	pub enum Event<T: Config> {}

	#[pallet::error]
	pub enum Error<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub stake_manager_address: ConfigItem,
		pub key_manager_address: ConfigItem,
		pub ethereum_chain_id: ConfigItem,
		pub ethereum_vault_address: ConfigItem,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				stake_manager_address: Default::default(),
				key_manager_address: Default::default(),
				ethereum_chain_id: Default::default(),
				ethereum_vault_address: Default::default(),
			}
		}
	}

	/// Sets the genesis governance
	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			StakeManagerAddress::<T>::set(self.stake_manager_address);
			KeyManagerAddress::<T>::set(self.key_manager_address);
			EthereumChainId::<T>::set(self.ethereum_chain_id);
			EthereumVaultAddress::<T>::set(self.ethereum_vault_address);
		}
	}
}
