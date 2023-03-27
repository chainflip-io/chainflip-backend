#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

use cf_chains::{
	btc::{
		api::SelectedUtxos, utxo_selection::select_utxos_from_pool, Bitcoin, BitcoinNetwork, Utxo,
	},
	dot::{api::CreatePolkadotVault, Polkadot, PolkadotAccountId, PolkadotHash, PolkadotIndex},
	ChainCrypto,
};
use cf_primitives::{Asset, BroadcastId, EthereumAddress};
pub use cf_traits::EthEnvironmentProvider;
use cf_traits::{SystemStateInfo, SystemStateManager};
use frame_support::{
	pallet_prelude::*,
	traits::{OnRuntimeUpgrade, StorageVersion},
};
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_std::vec::Vec;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod weights;
pub use weights::WeightInfo;
mod migrations;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(2);

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

	use cf_chains::{
		btc::{Utxo, UtxoId},
		dot::{PolkadotPublicKey, RuntimeVersion},
	};
	use cf_primitives::{Asset, TxId};

	use cf_traits::{BroadcastCleanup, Broadcaster, VaultKeyWitnessedHandler};

	use super::*;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: cf_traits::Chainflip {
		/// Because we want to emit events when there is a config change during
		/// an runtime upgrade
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// Governance origin to secure extrinsic
		type EnsureGovernance: EnsureOrigin<Self::RuntimeOrigin>;
		/// Polkadot Vault Creation Apicall
		type CreatePolkadotVault: CreatePolkadotVault;
		/// Polkadot broadcaster
		type PolkadotBroadcaster: Broadcaster<Polkadot, ApiCall = Self::CreatePolkadotVault>
			+ BroadcastCleanup<Polkadot>;
		/// On new key witnessed handler for Polkadot
		type PolkadotVaultKeyWitnessedHandler: VaultKeyWitnessedHandler<Polkadot>;
		/// On new key witnessed handler for Bitcoin
		type BitcoinVaultKeyWitnessedHandler: VaultKeyWitnessedHandler<Bitcoin>;

		#[pallet::constant]
		type BitcoinNetwork: Get<BitcoinNetwork>;

		/// Weight information
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The network is currently paused.
		NetworkIsInMaintenance,
		/// The settings provided were invalid.
		InvalidCfeSettings,
		/// Eth is not an Erc20 token, so its address can't be updated.
		EthAddressNotUpdateable,
		/// Polkadot runtime version is lower than the currently stored version.
		InvalidPolkadotRuntimeVersion,
	}

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(PhantomData<T>);

	// CHAINFLIP RELATED ENVIRONMENT ITEMS
	#[pallet::storage]
	#[pallet::getter(fn cfe_settings)]
	/// The settings used by the CFE
	pub type CfeSettings<T> = StorageValue<_, cfe::CfeSettings, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn system_state)]
	/// The current state the system is in (normal, maintenance).
	pub type CurrentSystemState<T> = StorageValue<_, SystemState, ValueQuery>;

	// ETHEREUM CHAIN RELATED ENVIRONMENT ITEMS
	#[pallet::storage]
	#[pallet::getter(fn supported_eth_assets)]
	/// Map of supported assets for ETH
	pub type EthereumSupportedAssets<T: Config> =
		StorageMap<_, Blake2_128Concat, Asset, EthereumAddress>;

	#[pallet::storage]
	#[pallet::getter(fn stake_manager_address)]
	/// The address of the ETH stake manager contract
	pub type EthereumStakeManagerAddress<T> = StorageValue<_, EthereumAddress, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn key_manager_address)]
	/// The address of the ETH key manager contract
	pub type EthereumKeyManagerAddress<T> = StorageValue<_, EthereumAddress, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn eth_vault_address)]
	/// The address of the ETH vault contract
	pub type EthereumVaultAddress<T> = StorageValue<_, EthereumAddress, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn ethereum_chain_id)]
	/// The ETH chain id
	pub type EthereumChainId<T> = StorageValue<_, u64, ValueQuery>;

	#[pallet::storage]
	pub type EthereumSignatureNonce<T> = StorageValue<_, SignatureNonce, ValueQuery>;

	// POLKADOT CHAIN RELATED ENVIRONMENT ITEMS

	#[pallet::storage]
	#[pallet::getter(fn polkadot_genesis_hash)]
	pub type PolkadotGenesisHash<T> = StorageValue<_, PolkadotHash, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn polkadot_vault_account)]
	/// The Polkadot Vault Anonymous Account
	pub type PolkadotVaultAccountId<T> = StorageValue<_, PolkadotAccountId, OptionQuery>;

	#[pallet::storage]
	/// Current Nonce of the current Polkadot Proxy Account
	pub type PolkadotProxyAccountNonce<T> = StorageValue<_, PolkadotIndex, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn polkadot_runtime_version)]
	pub type PolkadotRuntimeVersion<T> = StorageValue<_, RuntimeVersion, ValueQuery>;

	// BITCOIN CHAIN RELATED ENVIRONMENT ITEMS
	#[pallet::storage]
	/// The set of available UTXOs available in our Bitcoin Vault.
	pub type BitcoinAvailableUtxos<T> = StorageValue<_, Vec<Utxo>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn bitcoin_network)]
	/// Selection of the bitcoin network (mainnet, testnet or regtest) that the state chain
	/// currently supports.
	pub type BitcoinNetworkSelection<T> = StorageValue<_, BitcoinNetwork, ValueQuery>;

	#[pallet::storage]
	/// The amount of fee we want to pay per utxo.
	pub type BitcoinFeePerUtxo<T> = StorageValue<_, u64, ValueQuery>;

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
		/// Polkadot Vault Creation Call was initiated
		PolkadotVaultCreationCallInitiated { agg_key: <Polkadot as ChainCrypto>::AggKey },
		/// Polkadot Vault Account is successfully set
		PolkadotVaultAccountSet { polkadot_vault_account_id: PolkadotAccountId },
		/// The Polkadot Runtime Version stored on chain was updated.
		PolkadotRuntimeVersionUpdated { runtime_version: RuntimeVersion },
		/// The block number for set for new Bitcoin vault
		BitcoinBlockNumberSetForVault { block_number: cf_chains::btc::BlockNumber },
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<T::BlockNumber> for Pallet<T> {
		fn on_runtime_upgrade() -> Weight {
			migrations::PalletMigration::<T>::on_runtime_upgrade()
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<sp_std::vec::Vec<u8>, &'static str> {
			migrations::PalletMigration::<T>::pre_upgrade()
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: sp_std::vec::Vec<u8>) -> Result<(), &'static str> {
			migrations::PalletMigration::<T>::post_upgrade(state)
		}
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
		/// - [EthereumSupportedAssetsUpdated](Event::EthereumSupportedAssetsUpdated)
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
			Self::deposit_event(if EthereumSupportedAssets::<T>::contains_key(asset) {
				EthereumSupportedAssets::<T>::mutate(asset, |new_address| {
					*new_address = Some(address)
				});
				Event::UpdatedEthAsset(asset, address)
			} else {
				EthereumSupportedAssets::<T>::insert(asset, address);
				Event::AddedNewEthAsset(asset, address)
			});
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

		/// Initiates the Polkadot Vault Creation Apicall. This governance action needs to be called
		/// when the first rotation is initiated after polkadot activation. The rotation will stall
		/// after keygen is completed and emit the event AwaitingGovernanceAction after which this
		/// governance extrinsic needs to be called
		///
		/// ## Events
		///
		/// - [PolkadotVaultCreationCallInitiated](Event::PolkadotVaultCreationCallInitiated)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_support::error::BadOrigin)
		#[allow(unused_variables)]
		#[pallet::weight(0)]
		pub fn create_polkadot_vault(
			origin: OriginFor<T>,
			dot_aggkey: PolkadotPublicKey,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;

			T::PolkadotBroadcaster::threshold_sign_and_broadcast(
				T::CreatePolkadotVault::new_unsigned(dot_aggkey),
			);
			Self::deposit_event(Event::<T>::PolkadotVaultCreationCallInitiated {
				agg_key: dot_aggkey,
			});
			Ok(().into())
		}

		/// Manually initiates Polkadot vault key rotation completion steps so Epoch rotation can be
		/// continued and sets the Polkadot Pure Proxy Vault in environment pallet. The extrinsic
		/// takes in the dot_pure_proxy_vault_key, which is obtained from the Polkadot blockchain as
		/// a result of creating a polkadot vault which is done by executing the extrinsic
		/// create_polkadot_vault(), dot_witnessed_aggkey, the aggkey which initiated the polkadot
		/// creation transaction and the tx hash and block number of the Polkadot block the
		/// vault creation transaction was witnessed in. This extrinsic should complete the Polkadot
		/// initiation process and the vault should rotate successfully.
		///
		/// ## Events
		///
		/// - [PolkadotVaultCreationCallInitiated](Event::PolkadotVaultCreationCallInitiated)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_support::error::BadOrigin)
		#[allow(unused_variables)]
		#[pallet::weight(0)]
		pub fn witness_polkadot_vault_creation(
			origin: OriginFor<T>,
			dot_pure_proxy_vault_key: PolkadotAccountId,
			dot_witnessed_aggkey: PolkadotPublicKey,
			tx_id: TxId,
			broadcast_id: BroadcastId,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;

			use cf_traits::VaultKeyWitnessedHandler;

			// Set Polkadot Pure Proxy Vault Account
			PolkadotVaultAccountId::<T>::put(dot_pure_proxy_vault_key.clone());
			Self::deposit_event(Event::<T>::PolkadotVaultAccountSet {
				polkadot_vault_account_id: dot_pure_proxy_vault_key,
			});

			// Witness the agg_key rotation manually in the vaults pallet for polkadot
			let dispatch_result = T::PolkadotVaultKeyWitnessedHandler::on_new_key_activated(
				dot_witnessed_aggkey,
				tx_id.block_number,
				tx_id,
			)?;
			// Clean up the broadcast state.
			T::PolkadotBroadcaster::clean_up_broadcast(broadcast_id)?;

			Self::next_polkadot_proxy_account_nonce();
			Ok(dispatch_result)
		}

		/// Manually witnesses the current Bitcoin block number to complete the pending vault
		/// rotation.
		///
		/// ## Events
		///
		/// - [BitcoinBlockNumberSetForVault](Event::BitcoinBlockNumberSetForVault)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_support::error::BadOrigin)
		#[allow(unused_variables)]
		#[pallet::weight(0)]
		pub fn witness_current_bitcoin_block_number_for_key(
			origin: OriginFor<T>,
			block_number: cf_chains::btc::BlockNumber,
			new_public_key: cf_chains::btc::AggKey,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;

			use cf_traits::VaultKeyWitnessedHandler;

			// Witness the agg_key rotation manually in the vaults pallet for bitcoin
			let dispatch_result = T::BitcoinVaultKeyWitnessedHandler::on_new_key_activated(
				new_public_key,
				block_number,
				UtxoId {
					tx_hash: Default::default(),
					vout: Default::default(),
					pubkey_x: Default::default(),
					salt: Default::default(),
				},
			)?;

			Self::deposit_event(Event::<T>::BitcoinBlockNumberSetForVault { block_number });

			Ok(dispatch_result)
		}

		#[pallet::weight(T::WeightInfo::update_polkadot_runtime_version())]
		pub fn update_polkadot_runtime_version(
			origin: OriginFor<T>,
			runtime_version: RuntimeVersion,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;

			// If the `transaction_version` is bumped, the `spec_version` must also be bumped.
			// So we only need to check the `spec_version` here.
			// https://paritytech.github.io/substrate/master/sp_version/struct.RuntimeVersion.html#structfield.transaction_version
			ensure!(
				runtime_version.spec_version > PolkadotRuntimeVersion::<T>::get().spec_version,
				Error::<T>::InvalidPolkadotRuntimeVersion
			);

			PolkadotRuntimeVersion::<T>::put(runtime_version);
			Self::deposit_event(Event::<T>::PolkadotRuntimeVersionUpdated { runtime_version });

			Ok(().into())
		}

		#[pallet::weight(0)]
		pub fn add_btc_change_utxos(
			origin: OriginFor<T>,
			utxos: Vec<cf_chains::btc::Utxo>,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;

			for utxo in utxos {
				Self::add_bitcoin_utxo_to_list(utxo);
			}

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
		pub polkadot_genesis_hash: PolkadotHash,
		pub polkadot_vault_account_id: Option<PolkadotAccountId>,
		pub polkadot_runtime_version: RuntimeVersion,
		pub bitcoin_network: BitcoinNetwork,
		pub bitcoin_fee_per_utxo: u64,
	}

	/// Sets the genesis config
	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig {
		fn build(&self) {
			EthereumStakeManagerAddress::<T>::set(self.stake_manager_address);
			EthereumKeyManagerAddress::<T>::set(self.key_manager_address);
			EthereumVaultAddress::<T>::set(self.eth_vault_address);
			EthereumChainId::<T>::set(self.ethereum_chain_id);
			CfeSettings::<T>::set(self.cfe_settings);
			CurrentSystemState::<T>::set(SystemState::Normal);
			EthereumSupportedAssets::<T>::insert(Asset::Flip, self.flip_token_address);
			EthereumSupportedAssets::<T>::insert(Asset::Usdc, self.eth_usdc_address);

			PolkadotGenesisHash::<T>::set(self.polkadot_genesis_hash);
			PolkadotVaultAccountId::<T>::set(self.polkadot_vault_account_id.clone());
			PolkadotRuntimeVersion::<T>::set(self.polkadot_runtime_version);
			PolkadotProxyAccountNonce::<T>::set(0);

			BitcoinAvailableUtxos::<T>::set(vec![]);
			BitcoinNetworkSelection::<T>::set(self.bitcoin_network);
			BitcoinFeePerUtxo::<T>::set(self.bitcoin_fee_per_utxo);
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
	fn token_address(asset: Asset) -> Option<EthereumAddress> {
		EthereumSupportedAssets::<T>::get(asset)
	}
	fn key_manager_address() -> EthereumAddress {
		EthereumKeyManagerAddress::<T>::get()
	}
	fn vault_address() -> EthereumAddress {
		EthereumVaultAddress::<T>::get()
	}
	fn stake_manager_address() -> EthereumAddress {
		EthereumStakeManagerAddress::<T>::get()
	}
	fn chain_id() -> u64 {
		EthereumChainId::<T>::get()
	}
}

impl<T: Config> Pallet<T> {
	pub fn next_ethereum_signature_nonce() -> SignatureNonce {
		EthereumSignatureNonce::<T>::mutate(|nonce| {
			*nonce += 1;
			*nonce
		})
	}

	pub fn next_polkadot_proxy_account_nonce() -> PolkadotIndex {
		PolkadotProxyAccountNonce::<T>::mutate(|nonce| {
			*nonce += 1;
			*nonce - 1
		})
	}

	pub fn reset_polkadot_proxy_account_nonce() {
		PolkadotProxyAccountNonce::<T>::set(0);
	}

	pub fn add_bitcoin_utxo_to_list(utxo: Utxo) {
		BitcoinAvailableUtxos::<T>::append(utxo);
	}

	// Calculate the selection of utxos, return them and remove them from the list. If the
	// total output amount exceeds the total spendable amount of all utxos, the function
	// selects all utxos. The fee required to spend the utxos are accounted for while selection.
	pub fn select_and_take_bitcoin_utxos(
		total_output_amount: cf_chains::btc::BtcAmount,
	) -> Option<SelectedUtxos> {
		BitcoinAvailableUtxos::<T>::mutate(|available_utxos| {
			select_utxos_from_pool(
				available_utxos,
				BitcoinFeePerUtxo::<T>::get(),
				total_output_amount.try_into().expect("Btc amounts never exceed u64 max, this is made shure elsewhere by the AMM when it calculates how much amounts to output"),
			)
		})
	}
}
