#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

use cf_chains::{
	btc::{
		api::{SelectedUtxosAndChangeAmount, UtxoSelectionType},
		deposit_address::DepositAddress,
		utxo_selection::{select_utxos_for_consolidation, select_utxos_from_pool},
		Bitcoin, BtcAmount, Utxo, UtxoId, CHANGE_ADDRESS_SALT,
	},
	dot::{Polkadot, PolkadotAccountId, PolkadotHash, PolkadotIndex},
	eth::Address as EthereumAddress,
};
use cf_primitives::{chains::assets::eth::Asset as EthAsset, NetworkEnvironment, SemVer};
use cf_traits::{CompatibleCfeVersions, GetBitcoinFeeInfo, NetworkEnvironmentProvider, SafeMode};
use frame_support::{pallet_prelude::*, traits::StorageVersion};
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_std::{vec, vec::Vec};

mod benchmarking;

mod mock;
mod tests;

pub mod weights;
pub use weights::WeightInfo;
pub mod migrations;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(7);

const INITIAL_CONSOLIDATION_PARAMETERS: cf_chains::btc::ConsolidationParameters =
	cf_chains::btc::ConsolidationParameters {
		consolidation_threshold: 200,
		consolidation_size: 100,
	};

type SignatureNonce = u64;

#[derive(
	Encode, Decode, MaxEncodedLen, TypeInfo, Clone, RuntimeDebugNoBound, PartialEq, Eq, Default,
)]
#[scale_info(skip_type_params(T))]
pub enum SafeModeUpdate<T: Config> {
	/// Sh*t, meet Fan. Stop everything.
	CodeRed,
	/// Sunshine, meet Rainbows. Regular operation.
	#[default]
	CodeGreen,
	/// Schrödinger, meet Cat. It's complicated.
	CodeAmber(T::RuntimeSafeMode),
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_chains::btc::Utxo;
	use cf_primitives::TxId;
	use cf_traits::VaultKeyWitnessedHandler;
	use frame_support::DefaultNoBound;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: cf_traits::Chainflip {
		/// Because we want to emit events when there is a config change during
		/// an runtime upgrade
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// On new key witnessed handler for Polkadot
		type PolkadotVaultKeyWitnessedHandler: VaultKeyWitnessedHandler<Polkadot>;
		/// On new key witnessed handler for Bitcoin
		type BitcoinVaultKeyWitnessedHandler: VaultKeyWitnessedHandler<Bitcoin>;

		/// The runtime's safe mode is stored in this pallet.
		type RuntimeSafeMode: cf_traits::SafeMode + Member + Parameter + Default;

		/// Get Bitcoin Fee info from chain tracking
		type BitcoinFeeInfo: cf_traits::GetBitcoinFeeInfo;

		/// Used to access the current Chainflip runtime's release version (distinct from the
		/// substrate RuntimeVersion)
		#[pallet::constant]
		type CurrentReleaseVersion: Get<SemVer>;

		/// Weight information
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Eth is not an Erc20 token, so its address can't be updated.
		EthAddressNotUpdateable,
	}

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(PhantomData<T>);

	// ETHEREUM CHAIN RELATED ENVIRONMENT ITEMS
	#[pallet::storage]
	#[pallet::getter(fn supported_eth_assets)]
	/// Map of supported assets for ETH
	pub type EthereumSupportedAssets<T: Config> =
		StorageMap<_, Blake2_128Concat, EthAsset, EthereumAddress>;

	#[pallet::storage]
	#[pallet::getter(fn state_chain_gateway_address)]
	/// The address of the ETH state chain gatweay contract
	pub type EthereumStateChainGatewayAddress<T> = StorageValue<_, EthereumAddress, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn key_manager_address)]
	/// The address of the ETH key manager contract
	pub type EthereumKeyManagerAddress<T> = StorageValue<_, EthereumAddress, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn eth_vault_address)]
	/// The address of the ETH vault contract
	pub type EthereumVaultAddress<T> = StorageValue<_, EthereumAddress, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn eth_address_checker_address)]
	/// The address of the Address Checker contract on ETH
	pub type EthereumAddressCheckerAddress<T> = StorageValue<_, EthereumAddress, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn ethereum_chain_id)]
	/// The ETH chain id
	pub type EthereumChainId<T> = StorageValue<_, cf_chains::evm::api::EvmChainId, ValueQuery>;

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

	// BITCOIN CHAIN RELATED ENVIRONMENT ITEMS
	#[pallet::storage]
	/// The set of available UTXOs available in our Bitcoin Vault.
	pub type BitcoinAvailableUtxos<T> = StorageValue<_, Vec<Utxo>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn consolidation_parameters)]
	pub type ConsolidationParameters<T> =
		StorageValue<_, cf_chains::btc::ConsolidationParameters, ValueQuery>;

	// OTHER ENVIRONMENT ITEMS
	#[pallet::storage]
	#[pallet::getter(fn safe_mode)]
	/// Stores the current safe mode state for the runtime.
	pub type RuntimeSafeMode<T> = StorageValue<_, <T as Config>::RuntimeSafeMode, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn current_release_version)]
	/// Always set to the current release version. We duplicate the `CurrentReleaseVersion` pallet
	/// constant to allow querying the value by block hash.
	pub type CurrentReleaseVersion<T> = StorageValue<_, SemVer, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn network_environment)]
	/// Contains the network environment for this runtime.
	pub type ChainflipNetworkEnvironment<T> = StorageValue<_, NetworkEnvironment, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new supported ETH asset was added
		AddedNewEthAsset(EthAsset, EthereumAddress),
		/// The address of an supported ETH asset was updated
		UpdatedEthAsset(EthAsset, EthereumAddress),
		/// Polkadot Vault Account is successfully set
		PolkadotVaultAccountSet { polkadot_vault_account_id: PolkadotAccountId },
		/// The starting block number for the new Bitcoin vault was set
		BitcoinBlockNumberSetForVault { block_number: cf_chains::btc::BlockNumber },
		/// The Safe Mode settings for the chain has been updated
		RuntimeSafeModeUpdated { safe_mode: SafeModeUpdate<T> },
		/// UTXO consolidation parameters has been updated
		UtxoConsolidationParametersUpdated { params: cf_chains::btc::ConsolidationParameters },
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
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
		#[pallet::call_index(1)]
		// This weight is not strictly correct but since it's a governance call, weight is
		// irrelevant.
		#[pallet::weight(Weight::zero())]
		pub fn witness_polkadot_vault_creation(
			origin: OriginFor<T>,
			dot_pure_proxy_vault_key: PolkadotAccountId,
			tx_id: TxId,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;

			use cf_traits::VaultKeyWitnessedHandler;

			// Set Polkadot Pure Proxy Vault Account
			PolkadotVaultAccountId::<T>::put(dot_pure_proxy_vault_key);
			Self::deposit_event(Event::<T>::PolkadotVaultAccountSet {
				polkadot_vault_account_id: dot_pure_proxy_vault_key,
			});

			// Witness the agg_key rotation manually in the vaults pallet for polkadot
			let dispatch_result =
				T::PolkadotVaultKeyWitnessedHandler::on_new_key_activated(tx_id.block_number)?;

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
		#[pallet::call_index(2)]
		// This weight is not strictly correct but since it's a governance call, weight is
		// irrelevant.
		#[pallet::weight(Weight::zero())]
		pub fn witness_current_bitcoin_block_number_for_key(
			origin: OriginFor<T>,
			block_number: cf_chains::btc::BlockNumber,
			new_public_key: cf_chains::btc::AggKey,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;

			use cf_traits::VaultKeyWitnessedHandler;

			// Witness the agg_key rotation manually in the vaults pallet for bitcoin
			let dispatch_result =
				T::BitcoinVaultKeyWitnessedHandler::on_new_key_activated(block_number)?;

			Self::deposit_event(Event::<T>::BitcoinBlockNumberSetForVault { block_number });

			Ok(dispatch_result)
		}

		/// Update the current safe mode status.
		///
		/// Can only be dispatched from the governance origin.
		///
		/// See [SafeModeUpdate] for the different options.
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::update_safe_mode())]
		pub fn update_safe_mode(origin: OriginFor<T>, update: SafeModeUpdate<T>) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			RuntimeSafeMode::<T>::put(match update.clone() {
				SafeModeUpdate::CodeGreen => SafeMode::CODE_GREEN,
				SafeModeUpdate::CodeRed => SafeMode::CODE_RED,
				SafeModeUpdate::CodeAmber(safe_mode) => safe_mode,
			});

			Self::deposit_event(Event::<T>::RuntimeSafeModeUpdated { safe_mode: update });

			Ok(())
		}

		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::update_consolidation_parameters())]
		pub fn update_consolidation_parameters(
			origin: OriginFor<T>,
			params: cf_chains::btc::ConsolidationParameters,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			ensure!(params.are_valid(), DispatchError::Other("Invalid parameters"));

			ConsolidationParameters::<T>::set(params);

			Self::deposit_event(Event::<T>::UtxoConsolidationParametersUpdated { params });

			Ok(())
		}
	}

	#[pallet::genesis_config]
	#[derive(DefaultNoBound)]
	pub struct GenesisConfig<T> {
		pub flip_token_address: EthereumAddress,
		pub eth_usdc_address: EthereumAddress,
		pub state_chain_gateway_address: EthereumAddress,
		pub key_manager_address: EthereumAddress,
		pub eth_vault_address: EthereumAddress,
		pub eth_address_checker_address: EthereumAddress,
		pub ethereum_chain_id: u64,
		pub polkadot_genesis_hash: PolkadotHash,
		pub polkadot_vault_account_id: Option<PolkadotAccountId>,
		pub network_environment: NetworkEnvironment,
		pub _config: PhantomData<T>,
	}

	/// Sets the genesis config
	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			EthereumStateChainGatewayAddress::<T>::set(self.state_chain_gateway_address);
			EthereumKeyManagerAddress::<T>::set(self.key_manager_address);
			EthereumVaultAddress::<T>::set(self.eth_vault_address);
			EthereumAddressCheckerAddress::<T>::set(self.eth_address_checker_address);

			EthereumChainId::<T>::set(self.ethereum_chain_id);
			EthereumSupportedAssets::<T>::insert(EthAsset::Flip, self.flip_token_address);
			EthereumSupportedAssets::<T>::insert(EthAsset::Usdc, self.eth_usdc_address);

			PolkadotGenesisHash::<T>::set(self.polkadot_genesis_hash);
			PolkadotVaultAccountId::<T>::set(self.polkadot_vault_account_id);
			PolkadotProxyAccountNonce::<T>::set(0);

			BitcoinAvailableUtxos::<T>::set(vec![]);
			ConsolidationParameters::<T>::set(INITIAL_CONSOLIDATION_PARAMETERS);

			ChainflipNetworkEnvironment::<T>::set(self.network_environment);

			Pallet::<T>::update_current_release_version();
		}
	}
}

impl<T: Config> Pallet<T> {
	pub fn update_current_release_version() {
		CurrentReleaseVersion::<T>::set(T::CurrentReleaseVersion::get());
	}

	pub fn next_ethereum_signature_nonce() -> SignatureNonce {
		EthereumSignatureNonce::<T>::mutate(|nonce| {
			*nonce += 1;
			*nonce
		})
	}

	pub fn next_polkadot_proxy_account_nonce(reset_nonce: bool) -> PolkadotIndex {
		PolkadotProxyAccountNonce::<T>::mutate(|nonce| {
			let current_nonce = *nonce;

			if reset_nonce {
				*nonce = 0;
			} else {
				*nonce += 1;
			}
			current_nonce
		})
	}

	pub fn add_bitcoin_utxo_to_list(
		amount: BtcAmount,
		utxo_id: UtxoId,
		deposit_address: DepositAddress,
	) {
		BitcoinAvailableUtxos::<T>::append(Utxo { amount, id: utxo_id, deposit_address });
	}

	pub fn add_bitcoin_change_utxo(amount: BtcAmount, utxo_id: UtxoId, pubkey_x: [u8; 32]) {
		BitcoinAvailableUtxos::<T>::append(Utxo {
			amount,
			id: utxo_id,
			deposit_address: DepositAddress::new(pubkey_x, CHANGE_ADDRESS_SALT),
		});
	}

	// Calculate the selection of utxos, return them and remove them from the list. The fee required
	// to spend the input utxos are accounted for while selection. The fee required to include
	// outputs and the minimum constant tx fee is incorporated by adding to the output amount. The
	// function returns the selected Utxos and the change amount that remains from the selected
	// input Utxo list once outputs and the tx fees have been taken into account.
	pub fn select_and_take_bitcoin_utxos(
		utxo_selection_type: UtxoSelectionType,
	) -> Option<SelectedUtxosAndChangeAmount> {
		let bitcoin_fee_info = T::BitcoinFeeInfo::bitcoin_fee_info();
		let fee_per_input_utxo = bitcoin_fee_info.fee_per_input_utxo();
		let min_fee_required_per_tx = bitcoin_fee_info.min_fee_required_per_tx();
		let fee_per_output_utxo = bitcoin_fee_info.fee_per_output_utxo();

		match utxo_selection_type {
			UtxoSelectionType::SelectAllForRotation => {
				let spendable_utxos: Vec<_> = BitcoinAvailableUtxos::<T>::take()
					.into_iter()
					.filter(|utxo| utxo.amount > fee_per_input_utxo)
					.collect();

				if spendable_utxos.is_empty() {
					return None
				}

				let total_fee = spendable_utxos.len() as u64 * fee_per_input_utxo +
					fee_per_output_utxo + min_fee_required_per_tx;

				spendable_utxos
					.iter()
					.map(|utxo| utxo.amount)
					.sum::<u64>()
					.checked_sub(total_fee)
					.map(|change_amount| (spendable_utxos, change_amount))
			},
			UtxoSelectionType::SelectForConsolidation =>
				BitcoinAvailableUtxos::<T>::try_mutate(|available_utxos| {
					let params = Self::consolidation_parameters();

					let utxos_to_consolidate =
						select_utxos_for_consolidation(available_utxos, fee_per_input_utxo, params);

					if utxos_to_consolidate.is_empty() {
						Err(())
					} else {
						let total_fee = utxos_to_consolidate.len() as u64 * fee_per_input_utxo +
							fee_per_output_utxo + min_fee_required_per_tx;

						utxos_to_consolidate
							.iter()
							.map(|utxo| utxo.amount)
							.sum::<u64>()
							.checked_sub(total_fee)
							.map(|change_amount| (utxos_to_consolidate, change_amount))
							.ok_or(())
					}
				})
				.ok(),
			UtxoSelectionType::Some { output_amount, number_of_outputs } =>
				BitcoinAvailableUtxos::<T>::try_mutate(|available_utxos| {
					select_utxos_from_pool(
						available_utxos,
						fee_per_input_utxo,
						output_amount +
							number_of_outputs * fee_per_output_utxo +
							min_fee_required_per_tx,
					)
					.ok_or_else(|| {
						log::error!("Unable to select desired amount from available utxos.");
					})
				})
				.ok()
				.map(|(selected_utxos, total_input_spendable_amount)| {
					(
						selected_utxos,
						total_input_spendable_amount -
							output_amount - number_of_outputs * fee_per_output_utxo -
							min_fee_required_per_tx,
					)
				}),
		}
	}
}

impl<T: Config> CompatibleCfeVersions for Pallet<T> {
	fn current_release_version() -> SemVer {
		Self::current_release_version()
	}
}

impl<T: Config> NetworkEnvironmentProvider for Pallet<T> {
	fn get_network_environment() -> NetworkEnvironment {
		Self::network_environment()
	}
}
