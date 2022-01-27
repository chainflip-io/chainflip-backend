#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weights;
pub use weights::WeightInfo;

use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;

pub mod cfe {
	use super::*;
	/// On chain CFE settings
	#[derive(Encode, Decode, Clone, RuntimeDebug, Default, PartialEq, Eq, Copy)]
	pub struct CfeSettings {
		/// Number of blocks we wait until we consider the ethereum witnesser stream finalized.
		pub eth_block_safety_margin: u32,
		/// Defines how long a signing ceremony remains pending in seconds. i.e. how long it waits
		/// for the key that is supposed to sign this message to be generated. (Since we can
		/// receive requests to sign for the next key, if other nodes are ahead of us)
		pub pending_sign_duration_secs: u32,
		/// Maximum duration a ceremony stage can last in seconds
		pub max_ceremony_stage_duration_secs: u32,
		/// Number of times to retry after incrementing the nonce on a nonce error
		pub max_extrinsic_retry_attempts: u32,
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
	/// The settings used by the CFE
	pub type CfeSettings<T> = StorageValue<_, cfe::CfeSettings, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		UpdatedCFESettings(cfe::CfeSettings),
	}

	#[pallet::error]
	pub enum Error<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Sets the value for the CFE setting EthBlockSafetyMargin
		///
		/// ## Events
		///
		/// - [UpdatedCFESettings](Event::UpdatedCFESettings)
		#[pallet::weight(T::WeightInfo::update_eth_block_safety_margin())]
		pub fn update_eth_block_safety_margin(
			origin: OriginFor<T>,
			eth_block_safety_margin: u32,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;
			Self::do_cfe_config_update(|settings: &mut cfe::CfeSettings| {
				settings.eth_block_safety_margin = eth_block_safety_margin;
			});
			Ok(().into())
		}
		/// Sets the value for the CFE setting PendingSignDuration
		///
		/// ## Events
		///
		/// - [UpdatedCFESettings](Event::UpdatedCFESettings)
		#[pallet::weight(T::WeightInfo::update_max_retry_attempts())]
		pub fn update_max_retry_attempts(
			origin: OriginFor<T>,
			max_extrinsic_retry_attempts: u32,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;
			Self::do_cfe_config_update(|settings: &mut cfe::CfeSettings| {
				settings.max_extrinsic_retry_attempts = max_extrinsic_retry_attempts;
			});
			Ok(().into())
		}
		/// Sets the value for the CFE setting MaxStageDuration
		///
		/// ## Events
		///
		/// - [UpdatedCFESettings](Event::UpdatedCFESettings)
		#[pallet::weight(T::WeightInfo::update_max_stage_duration())]
		pub fn update_max_stage_duration(
			origin: OriginFor<T>,
			max_ceremony_stage_duration: u32,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;
			Self::do_cfe_config_update(|settings: &mut cfe::CfeSettings| {
				settings.max_ceremony_stage_duration_secs = max_ceremony_stage_duration_secs;
			});
			Ok(().into())
		}
		/// Sets the value for the CFE setting MaxRetryAttempts
		///
		/// ## Events
		///
		/// - [UpdatedCFESettings](Event::UpdatedCFESettings)
		#[pallet::weight(T::WeightInfo::update_pending_sign_duration())]
		pub fn update_pending_sign_duration(
			origin: OriginFor<T>,
			pending_sign_duration: u32,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;
			Self::do_cfe_config_update(&|settings: &mut cfe::CfeSettings| {
				settings.pending_sign_duration_secs = pending_sign_duration_secs;
				*settings
			});
			Ok(().into())
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig {
		pub stake_manager_address: EthereumAddress,
		pub key_manager_address: EthereumAddress,
		pub ethereum_chain_id: u64,
		pub eth_block_safety_margin: u32,
		pub pending_sign_duration_secs: u32,
		pub max_ceremony_stage_duration_secs: u32,
		pub max_extrinsic_retry_attempts: u32,
	}

	#[cfg(feature = "std")]
	impl Default for GenesisConfig {
		fn default() -> Self {
			Self {
				stake_manager_address: Default::default(),
				key_manager_address: Default::default(),
				ethereum_chain_id: Default::default(),
				eth_block_safety_margin: Default::default(),
				pending_sign_duration_secs: Default::default(),
				max_ceremony_stage_duration_secs: Default::default(),
				max_extrinsic_retry_attempts: Default::default(),
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
				pending_sign_duration_secs: self.pending_sign_duration_secs,
				max_ceremony_stage_duration_secs: self.max_ceremony_stage_duration_secs,
				max_extrinsic_retry_attempts: self.max_extrinsic_retry_attempts,
			});
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Updates the cfe settings and emits an event with the updated values
	fn do_cfe_config_update(update_settings: impl Fn(&mut cfe::CfeSettings)) {
		let new_settings = CfeSettings::<T>::mutate(|settings| {
			update_settings(settings);
			*settings
		});
		Self::deposit_event(Event::UpdatedCFESettings(new_settings));
	}
}
