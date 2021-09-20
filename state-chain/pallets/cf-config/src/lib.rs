#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;
#[frame_support::pallet]
pub mod pallet {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;
	use sp_std::vec::Vec;

	type ConfigItem = Vec<u8>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
	}
	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

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
	pub struct GenesisConfig {
		pub stake_manager_address: ConfigItem,
		pub key_manager_address: ConfigItem,
		pub ethereum_chain_id: ConfigItem,
		pub ethereum_vault_address: ConfigItem,
	}

	#[cfg(feature = "std")]
	impl Default for GenesisConfig {
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
	impl<T: Config> GenesisBuild<T> for GenesisConfig {
		fn build(&self) {
			StakeManagerAddress::<T>::set(Some(self.stake_manager_address.clone()));
			KeyManagerAddress::<T>::set(Some(self.key_manager_address.clone()));
			EthereumChainId::<T>::set(Some(self.ethereum_chain_id.clone()));
			EthereumVaultAddress::<T>::set(Some(self.ethereum_vault_address.clone()));
		}
	}
}
