#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

use cf_primitives::{Asset, EthereumAddress};
pub use cf_traits::EthEnvironmentProvider;
use cf_traits::{SystemStateInfo, SystemStateManager};
use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;
pub use pallet::*;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod weights;
pub use weights::WeightInfo;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum SystemState {
	Normal,
	Maintenance,
}

impl Default for SystemState {
	fn default() -> Self {
		SystemState::Normal
	}
}
type SignatureNonce = u64;

pub mod cfe {
	use super::*;
	/// On chain CFE settings
	#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, Clone, RuntimeDebug, PartialEq, Eq, Copy)]
	#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
	pub struct CfeSettings {
		/// Number of blocks we wait until we consider the ethereum witnesser stream finalized.
		pub eth_block_safety_margin: u32,
		/// The percentile of priority fee we want to fetch from fee_history (expressed
		/// as a number between 0 and 100)
		pub eth_priority_fee_percentile: u8,
		/// Maximum duration a ceremony stage can last
		pub max_ceremony_stage_duration: u32,
	}

	/// Sensible default values for the CFE setting.
	impl Default for CfeSettings {
		fn default() -> Self {
			Self {
				eth_block_safety_margin: 6,
				eth_priority_fee_percentile: 50,
				max_ceremony_stage_duration: 300,
			}
		}
	}
}

#[frame_support::pallet]
pub mod pallet {
	use cf_primitives::Asset;

	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Because we want to emit events when there is a config change during
		/// an runtime upgrade
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		/// Governance origin to secure extrinsic
		type EnsureGovernance: EnsureOrigin<Self::Origin>;
		/// Weight information
		type WeightInfo: WeightInfo;
		/// Eth Environment provider
		type EthEnvironmentProvider: EthEnvironmentProvider;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The network is currently paused.
		NetworkIsInMaintenance,
		/// The settings provided were invalid.
		InvalidCfeSettings,
		/// Eth is not an Erc20 token, so its address can't be updated.
		EthAddressNotUpdateable,
	}

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::storage]
	#[pallet::getter(fn supported_eth_assets)]
	/// Map of supported assets for ETH
	pub type SupportedEthAssets<T: Config> =
		StorageMap<_, Blake2_128Concat, Asset, EthereumAddress>;

	#[pallet::storage]
	#[pallet::getter(fn stake_manager_address)]
	/// The address of the ETH stake manager contract
	pub type StakeManagerAddress<T> = StorageValue<_, EthereumAddress, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn key_manager_address)]
	/// The address of the ETH key manager contract
	pub type KeyManagerAddress<T> = StorageValue<_, EthereumAddress, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn eth_vault_address)]
	/// The address of the ETH vault contract
	pub type EthVaultAddress<T> = StorageValue<_, EthereumAddress, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn ethereum_chain_id)]
	/// The address of the ETH chain id
	pub type EthereumChainId<T> = StorageValue<_, u64, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn cfe_settings)]
	/// The settings used by the CFE
	pub type CfeSettings<T> = StorageValue<_, cfe::CfeSettings, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn system_state)]
	/// The current state the system is in (normal, maintenance).
	pub type CurrentSystemState<T> = StorageValue<_, SystemState, ValueQuery>;

	#[pallet::storage]
	pub type GlobalSignatureNonce<T> = StorageValue<_, SignatureNonce, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// The system state has been updated
		SystemStateUpdated { new_system_state: SystemState },
		/// The on-chain CFE settings have been updated
		CfeSettingsUpdated { new_cfe_settings: cfe::CfeSettings },
		/// A new supported ETH asset was added
		AddedNewEthAsset(Asset, EthereumAddress),
		/// The address of an supported ETH asset was updated
		UpdatedEthAsset(Asset, EthereumAddress),
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Changes the current system state.
		///
		/// ## Events
		///
		/// - [SystemStateUpdated](Event::SystemStateUpdated)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_support::error::BadOrigin)
		#[pallet::weight(T::WeightInfo::set_system_state())]
		pub fn set_system_state(
			origin: OriginFor<T>,
			state: SystemState,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;
			SystemStateProvider::<T>::set_system_state(state);
			Ok(().into())
		}
		/// Adds or updates an asset address in the map of supported ETH assets.
		///
		/// ## Events
		///
		/// - [SupportedEthAssetsUpdated](Event::SupportedEthAssetsUpdated)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_support::error::BadOrigin)
		#[pallet::weight(T::WeightInfo::update_supported_eth_assets())]
		pub fn update_supported_eth_assets(
			origin: OriginFor<T>,
			asset: Asset,
			address: EthereumAddress,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;
			ensure!(asset != Asset::Eth, Error::<T>::EthAddressNotUpdateable);
			if SupportedEthAssets::<T>::contains_key(asset) {
				SupportedEthAssets::<T>::mutate(asset, |new_address| *new_address = Some(address));
				Self::deposit_event(Event::UpdatedEthAsset(asset, address));
			} else {
				SupportedEthAssets::<T>::insert(asset, address);
				Self::deposit_event(Event::AddedNewEthAsset(asset, address));
			}
			Ok(().into())
		}
		/// Sets the current on-chain CFE settings
		///
		/// ## Events
		///
		/// - [CfeSettingsUpdated](Event::CfeSettingsUpdated)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_support::error::BadOrigin)
		#[pallet::weight(T::WeightInfo::set_cfe_settings())]
		pub fn set_cfe_settings(
			origin: OriginFor<T>,
			cfe_settings: cfe::CfeSettings,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;
			ensure!(
				cfe_settings.eth_priority_fee_percentile <= 100,
				Error::<T>::InvalidCfeSettings
			);
			CfeSettings::<T>::put(cfe_settings);
			Self::deposit_event(Event::<T>::CfeSettingsUpdated { new_cfe_settings: cfe_settings });
			Ok(().into())
		}
	}

	#[pallet::genesis_config]
	#[cfg_attr(feature = "std", derive(Default))]
	pub struct GenesisConfig {
		pub flip_token_address: EthereumAddress,
		pub eth_usdc_address: EthereumAddress,
		pub stake_manager_address: EthereumAddress,
		pub key_manager_address: EthereumAddress,
		pub eth_vault_address: EthereumAddress,
		pub ethereum_chain_id: u64,
		pub cfe_settings: cfe::CfeSettings,
	}

	/// Sets the genesis config
	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig {
		fn build(&self) {
			StakeManagerAddress::<T>::set(self.stake_manager_address);
			KeyManagerAddress::<T>::set(self.key_manager_address);
			EthVaultAddress::<T>::set(self.eth_vault_address);
			EthereumChainId::<T>::set(self.ethereum_chain_id);
			CfeSettings::<T>::set(self.cfe_settings);
			CurrentSystemState::<T>::set(SystemState::Normal);
			SupportedEthAssets::<T>::insert(Asset::Flip, self.flip_token_address);
			SupportedEthAssets::<T>::insert(Asset::Usdc, self.eth_usdc_address);
		}
	}
}

pub struct SystemStateProvider<T>(PhantomData<T>);

impl<T: Config> SystemStateProvider<T> {
	fn set_system_state(new_system_state: SystemState) {
		if CurrentSystemState::<T>::get() != new_system_state {
			CurrentSystemState::<T>::put(&new_system_state);
			Pallet::<T>::deposit_event(Event::<T>::SystemStateUpdated { new_system_state });
		}
	}
}

impl<T: Config> SystemStateInfo for SystemStateProvider<T> {
	fn ensure_no_maintenance() -> frame_support::sp_runtime::DispatchResult {
		ensure!(
			<pallet::CurrentSystemState<T>>::get() != SystemState::Maintenance,
			Error::<T>::NetworkIsInMaintenance
		);
		Ok(())
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn activate_maintenance_mode() {
		<Self as SystemStateManager>::activate_maintenance_mode();
	}
}

impl<T: Config> SystemStateManager for SystemStateProvider<T> {
	fn activate_maintenance_mode() {
		Self::set_system_state(SystemState::Maintenance);
	}
}

impl<T: Config> EthEnvironmentProvider for Pallet<T> {
	fn flip_token_address() -> [u8; 20] {
		SupportedEthAssets::<T>::get(Asset::Flip).expect("FLIP address should be added at genesis")
	}
	fn key_manager_address() -> [u8; 20] {
		KeyManagerAddress::<T>::get()
	}
	fn eth_vault_address() -> [u8; 20] {
		EthVaultAddress::<T>::get()
	}
	fn stake_manager_address() -> [u8; 20] {
		StakeManagerAddress::<T>::get()
	}
	fn chain_id() -> u64 {
		EthereumChainId::<T>::get()
	}
}

impl<T: Config> Pallet<T> {
	pub fn next_global_signature_nonce() -> SignatureNonce {
		GlobalSignatureNonce::<T>::mutate(|nonce| {
			*nonce += 1;
			*nonce
		})
	}
}
