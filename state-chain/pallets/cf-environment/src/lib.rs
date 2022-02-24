#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;

pub mod cfe {
	use super::*;
	/// On chain CFE settings
	#[derive(Encode, Decode, Clone, RuntimeDebug, PartialEq, Eq, Copy)]
	pub struct CfeSettings {
		/// Number of blocks we wait until we consider the ethereum witnesser stream finalized.
		pub eth_block_safety_margin: u32,
		/// Defines how long a signing ceremony remains pending. i.e. how long it waits for the key
		/// that is supposed to sign this message to be generated. (Since we can receive requests
		/// to sign for the next key, if other nodes are ahead of us)
		pub pending_sign_duration: u32,
		/// Maximum duration a ceremony stage can last
		pub max_ceremony_stage_duration: u32,
		/// Number of times to retry after incrementing the nonce on a nonce error
		pub max_extrinsic_retry_attempts: u32,
	}

	/// Sensible default values for the CFE setting.
	impl Default for CfeSettings {
		fn default() -> Self {
			Self {
				eth_block_safety_margin: 6,
				pending_sign_duration: 500,
				max_ceremony_stage_duration: 300,
				max_extrinsic_retry_attempts: 10,
			}
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

	#[pallet::event]
	pub enum Event<T: Config> {}

	#[pallet::error]
	pub enum Error<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {}

	#[pallet::genesis_config]
	pub struct GenesisConfig {
		pub stake_manager_address: EthereumAddress,
		pub key_manager_address: EthereumAddress,
		pub ethereum_chain_id: u64,
		pub eth_block_safety_margin: u32,
		pub pending_sign_duration: u32,
		pub max_ceremony_stage_duration: u32,
		pub max_extrinsic_retry_attempts: u32,
	}

	#[cfg(feature = "std")]
	impl Default for GenesisConfig {
		fn default() -> Self {
			let default_cfe_settings = cfe::CfeSettings::default();
			Self {
				stake_manager_address: Default::default(),
				key_manager_address: Default::default(),
				ethereum_chain_id: Default::default(),
				eth_block_safety_margin: default_cfe_settings.eth_block_safety_margin,
				pending_sign_duration: default_cfe_settings.pending_sign_duration,
				max_ceremony_stage_duration: default_cfe_settings.max_ceremony_stage_duration,
				max_extrinsic_retry_attempts: default_cfe_settings.max_extrinsic_retry_attempts,
			}
		}
	}

	/// Sets the genesis config
	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig {
		fn build(&self) {
			StakeManagerAddress::<T>::set(self.stake_manager_address);
			KeyManagerAddress::<T>::set(self.key_manager_address);
			EthereumChainId::<T>::set(self.ethereum_chain_id);
			CfeSettings::<T>::set(cfe::CfeSettings {
				eth_block_safety_margin: self.eth_block_safety_margin,
				pending_sign_duration: self.pending_sign_duration,
				max_ceremony_stage_duration: self.max_ceremony_stage_duration,
				max_extrinsic_retry_attempts: self.max_extrinsic_retry_attempts,
			});
		}
	}
}
