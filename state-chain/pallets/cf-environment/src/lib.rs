#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

pub use cf_traits::EthEnvironmentProvider;
use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;

type KeyNonce = u64;

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

pub use pallet::*;
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
		/// Eth Environment provider
		type EthEnvironmentProvider: EthEnvironmentProvider;
	}
	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::storage]
	#[pallet::getter(fn flip_token_address)]
	/// The address of the ETH Flip token contract
	pub type FlipTokenAddress<T> = StorageValue<_, EthereumAddress, ValueQuery>;

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
	// The global key nonce counter
	// We don't need a getter for this, since we only ever use the next value
	pub type GlobalKeyNonce<T> = StorageValue<_, KeyNonce, ValueQuery>;

	#[pallet::event]
	pub enum Event<T: Config> {}

	#[pallet::error]
	pub enum Error<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {}

	#[pallet::genesis_config]
	#[cfg_attr(feature = "std", derive(Default))]
	pub struct GenesisConfig {
		pub flip_token_address: EthereumAddress,
		pub stake_manager_address: EthereumAddress,
		pub key_manager_address: EthereumAddress,
		pub ethereum_chain_id: u64,
		pub cfe_settings: cfe::CfeSettings,
	}

	/// Sets the genesis config
	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig {
		fn build(&self) {
			FlipTokenAddress::<T>::set(self.flip_token_address);
			StakeManagerAddress::<T>::set(self.stake_manager_address);
			KeyManagerAddress::<T>::set(self.key_manager_address);
			EthereumChainId::<T>::set(self.ethereum_chain_id);
			CfeSettings::<T>::set(self.cfe_settings);
		}
	}

	impl<T: Config> EthEnvironmentProvider for Pallet<T> {
		fn flip_token_address() -> [u8; 20] {
			FlipTokenAddress::<T>::get()
		}
		fn key_manager_address() -> [u8; 20] {
			KeyManagerAddress::<T>::get()
		}
		fn stake_manager_address() -> [u8; 20] {
			StakeManagerAddress::<T>::get()
		}
		fn chain_id() -> u64 {
			EthereumChainId::<T>::get()
		}
	}
}

impl<T: Config> Pallet<T> {
	pub fn next_global_key_nonce() -> KeyNonce {
		GlobalKeyNonce::<T>::mutate(|nonce| {
			*nonce += 1;
			*nonce
		})
	}
}
