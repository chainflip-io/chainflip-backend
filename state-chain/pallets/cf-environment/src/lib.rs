#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weights;
pub use weights::WeightInfo;

pub use pallet::*;
#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use sp_std::time::Duration;

	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	type EthereumAddress = [u8; 20];

	/// On chain CFE settings
	#[derive(Encode, Decode, Clone, RuntimeDebug, Default, PartialEq, Eq)]
	pub struct CFESettingsStruct {
		/// Number of blocks we wait until we deem it safe (from reorgs)
		pub eth_block_safety_margin: u64,
		/// Defines how long a signing ceremony remains pending. i.e. how long it waits for the key
		/// that is supposed to sign this message to be generated. (Since we can receive requests
		/// to sign for the next key, if other nodes are ahead of us)
		pub pending_sign_duration: Duration,
		/// Maximum duration a ceremony stage can last
		pub max_stage_duration: Duration,
		/// Number of times to retry after incrementing the nonce on a nonce error
		pub max_retry_attempts: u32,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Because we want to emit events when there is a config change during
		/// an runtime upgrade
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		/// Governance origin to secure extrinsic
		type EnsureGovernance: EnsureOrigin<Self::Origin>;
		/// Benchmark stuff
		type WeightInfo: WeightInfo;
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
	/// The address of the ETH chain id
	pub type CFESettings<T> = StorageValue<_, CFESettingsStruct, ValueQuery>;

	#[pallet::event]
	pub enum Event<T: Config> {
		UpdatedCFESettings(CFESettingsStruct),
	}

	#[pallet::error]
	pub enum Error<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Sets the value for the CFE config: eth_block_safety_margin
		///
		/// ## Events
		///
		/// - [UpdatedCFESettings](Event::UpdatedCFESettings)
		#[pallet::weight(T::WeightInfo::update_eth_block_safety_margin())]
		pub fn update_eth_block_safety_margin(
			origin: OriginFor<T>,
			value: u64,
		) -> DispatchResultWithPostInfo {
			// Ensure the extrinsic was executed by the governance
			T::EnsureGovernance::ensure_origin(origin)?;
			let mut settings = CFESettings::<T>::get();
			settings.eth_block_safety_margin = value;
			CFESettings::<T>::put(settings);
			Ok(().into())
		}
		/// Sets the value for the CFE config: pending_sign_duration
		///
		/// ## Events
		///
		/// - [UpdatedCFESettings](Event::UpdatedCFESettings)
		#[pallet::weight(T::WeightInfo::update_pending_sign_duration())]
		pub fn update_pending_sign_duration(
			origin: OriginFor<T>,
			value: Duration,
		) -> DispatchResultWithPostInfo {
			// Ensure the extrinsic was executed by the governance
			T::EnsureGovernance::ensure_origin(origin)?;
			let mut settings = CFESettings::<T>::get();
			settings.pending_sign_duration = value;
			CFESettings::<T>::put(settings);
			Ok(().into())
		}
		/// Sets the value for the CFE config: max_stage_duration
		///
		/// ## Events
		///
		/// - [UpdatedCFESettings](Event::UpdatedCFESettings)
		#[pallet::weight(T::WeightInfo::update_max_stage_duration())]
		pub fn update_max_stage_duration(
			origin: OriginFor<T>,
			value: Duration,
		) -> DispatchResultWithPostInfo {
			// Ensure the extrinsic was executed by the governance
			T::EnsureGovernance::ensure_origin(origin)?;
			let mut settings = CFESettings::<T>::get();
			settings.max_stage_duration = value;
			CFESettings::<T>::put(settings);
			Ok(().into())
		}
		/// Sets the value for the CFE config: max_retry_attempts
		///
		/// ## Events
		///
		/// - [UpdatedCFESettings](Event::UpdatedCFESettings)
		#[pallet::weight(T::WeightInfo::update_max_retry_attempts())]
		pub fn update_max_retry_attempts(
			origin: OriginFor<T>,
			value: u32,
		) -> DispatchResultWithPostInfo {
			// Ensure the extrinsic was executed by the governance
			T::EnsureGovernance::ensure_origin(origin)?;
			let mut settings = CFESettings::<T>::get();
			settings.max_retry_attempts = value;
			CFESettings::<T>::put(settings);
			Ok(().into())
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig {
		pub stake_manager_address: EthereumAddress,
		pub key_manager_address: EthereumAddress,
		pub ethereum_chain_id: u64,
		pub eth_block_safety_margin: u64,
		pub pending_sign_duration: u64,
		pub max_stage_duration: u64,
		pub max_retry_attempts: u32,
	}

	#[cfg(feature = "std")]
	impl Default for GenesisConfig {
		fn default() -> Self {
			Self {
				stake_manager_address: Default::default(),
				key_manager_address: Default::default(),
				ethereum_chain_id: Default::default(),
				eth_block_safety_margin: Default::default(),
				pending_sign_duration: Default::default(),
				max_stage_duration: Default::default(),
				max_retry_attempts: Default::default(),
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
			CFESettings::<T>::set(CFESettingsStruct {
				eth_block_safety_margin: self.eth_block_safety_margin,
				pending_sign_duration: Duration::from_secs(self.pending_sign_duration),
				max_stage_duration: Duration::from_secs(self.max_stage_duration),
				max_retry_attempts: self.max_retry_attempts,
			});
		}
	}
}
