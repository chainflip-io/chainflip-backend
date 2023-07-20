#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

use cf_chains::{
	btc::{
		api::{SelectedUtxosAndChangeAmount, UtxoSelectionType},
		deposit_address::DepositAddress,
		utxo_selection::select_utxos_from_pool,
		Bitcoin, BitcoinFeeInfo, BitcoinNetwork, BtcAmount, ScriptPubkey, Utxo, UtxoId,
		CHANGE_ADDRESS_SALT,
	},
	dot::{api::CreatePolkadotVault, Polkadot, PolkadotAccountId, PolkadotHash, PolkadotIndex},
	ChainCrypto,
};
use cf_primitives::{chains::assets::eth::Asset as EthAsset, BroadcastId, EthereumAddress};
use cf_traits::{GetBitcoinFeeInfo, SafeMode};
use frame_support::{
	pallet_prelude::*,
	traits::{OnRuntimeUpgrade, StorageVersion},
};
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_std::vec::Vec;

mod benchmarking;

mod mock;
mod tests;

pub mod weights;
pub use weights::WeightInfo;
mod migrations;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(2);

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
	use cf_chains::{
		btc::{ScriptPubkey, Utxo},
		dot::{PolkadotPublicKey, RuntimeVersion},
	};
	use cf_primitives::TxId;
	use cf_traits::{BroadcastCleanup, Broadcaster, VaultKeyWitnessedHandler};

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: cf_traits::Chainflip {
		/// Because we want to emit events when there is a config change during
		/// an runtime upgrade
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// Polkadot Vault Creation Apicall
		type CreatePolkadotVault: CreatePolkadotVault;
		/// Polkadot broadcaster
		type PolkadotBroadcaster: Broadcaster<Polkadot, ApiCall = Self::CreatePolkadotVault>
			+ BroadcastCleanup<Polkadot>;
		/// On new key witnessed handler for Polkadot
		type PolkadotVaultKeyWitnessedHandler: VaultKeyWitnessedHandler<Polkadot>;
		/// On new key witnessed handler for Bitcoin
		type BitcoinVaultKeyWitnessedHandler: VaultKeyWitnessedHandler<Bitcoin>;

		/// The runtime's safe mode is stored in this pallet.
		type RuntimeSafeMode: cf_traits::SafeMode + Member + Parameter + Default;

		#[pallet::constant]
		type BitcoinNetwork: Get<BitcoinNetwork>;

		/// Get Bitcoin Fee info from chain tracking
		type BitcoinFeeInfo: cf_traits::GetBitcoinFeeInfo;

		/// Weight information
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Eth is not an Erc20 token, so its address can't be updated.
		EthAddressNotUpdateable,
		/// Polkadot runtime version is lower than the currently stored version.
		InvalidPolkadotRuntimeVersion,
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
	pub type EthereumChainId<T> = StorageValue<_, cf_chains::eth::api::EthereumChainId, ValueQuery>;

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
	/// Lookup for determining which salt and pubkey the current deposit Bitcoin Script was created
	/// from.
	pub type BitcoinActiveDepositAddressDetails<T> =
		StorageMap<_, Twox64Concat, ScriptPubkey, (u32, [u8; 32]), ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn safe_mode)]
	/// Stores the current safe mode state for the runtime.
	pub type RuntimeSafeMode<T> = StorageValue<_, <T as Config>::RuntimeSafeMode, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new supported ETH asset was added
		AddedNewEthAsset(EthAsset, EthereumAddress),
		/// The address of an supported ETH asset was updated
		UpdatedEthAsset(EthAsset, EthereumAddress),
		/// Polkadot Vault Creation Call was initiated
		PolkadotVaultCreationCallInitiated { agg_key: <Polkadot as ChainCrypto>::AggKey },
		/// Polkadot Vault Account is successfully set
		PolkadotVaultAccountSet { polkadot_vault_account_id: PolkadotAccountId },
		/// The Polkadot Runtime Version stored on chain was updated.
		PolkadotRuntimeVersionUpdated { runtime_version: RuntimeVersion },
		/// The starting block number for the new Bitcoin vault was set
		BitcoinBlockNumberSetForVault { block_number: cf_chains::btc::BlockNumber },
		/// The Safe Mode settings for the chain has been updated
		RuntimeSafeModeUpdated { safe_mode: SafeModeUpdate<T> },
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
			asset: EthAsset,
			address: EthereumAddress,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;
			ensure!(asset != EthAsset::Eth, Error::<T>::EthAddressNotUpdateable);
			Self::deposit_event(if EthereumSupportedAssets::<T>::contains_key(asset) {
				EthereumSupportedAssets::<T>::mutate(asset, |mapped_address| {
					mapped_address.replace(address);
				});
				Event::UpdatedEthAsset(asset, address)
			} else {
				EthereumSupportedAssets::<T>::insert(asset, address);
				Event::AddedNewEthAsset(asset, address)
			});
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
				T::CreatePolkadotVault::new_unsigned(),
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
			PolkadotVaultAccountId::<T>::put(dot_pure_proxy_vault_key);
			Self::deposit_event(Event::<T>::PolkadotVaultAccountSet {
				polkadot_vault_account_id: dot_pure_proxy_vault_key,
			});

			// The initial polkadot vault creation is special in that the *new* aggkey submits the
			// creating transaction. So the aggkey account does not need to be reset.
			// However, `on_new_key_activated` indirectly resets the nonce. So we get it here and
			// then we can set it again below.
			let correct_nonce = PolkadotProxyAccountNonce::<T>::get();

			// Witness the agg_key rotation manually in the vaults pallet for polkadot
			let dispatch_result =
				T::PolkadotVaultKeyWitnessedHandler::on_new_key_activated(tx_id.block_number)?;
			// Clean up the broadcast state.
			T::PolkadotBroadcaster::clean_up_broadcast(broadcast_id)?;

			PolkadotProxyAccountNonce::<T>::set(correct_nonce);
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
			let dispatch_result =
				T::BitcoinVaultKeyWitnessedHandler::on_new_key_activated(block_number)?;

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

		/// Update the current safe mode status.
		///
		/// Can only be dispatched from the governance origin.
		///
		/// See [SafeModeUpdate] for the different options.
		#[pallet::weight(T::WeightInfo::update_safe_mode())]
		pub fn update_safe_mode(
			origin: OriginFor<T>,
			update: SafeModeUpdate<T>,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;

			RuntimeSafeMode::<T>::put(match update.clone() {
				SafeModeUpdate::CodeGreen => SafeMode::CODE_GREEN,
				SafeModeUpdate::CodeRed => SafeMode::CODE_RED,
				SafeModeUpdate::CodeAmber(safe_mode) => safe_mode,
			});

			Self::deposit_event(Event::<T>::RuntimeSafeModeUpdated { safe_mode: update });

			Ok(().into())
		}
	}

	#[pallet::genesis_config]
	#[cfg_attr(feature = "std", derive(Default))]
	pub struct GenesisConfig {
		pub flip_token_address: EthereumAddress,
		pub eth_usdc_address: EthereumAddress,
		pub state_chain_gateway_address: EthereumAddress,
		pub key_manager_address: EthereumAddress,
		pub eth_vault_address: EthereumAddress,
		pub eth_address_checker_address: EthereumAddress,
		pub ethereum_chain_id: u64,
		pub polkadot_genesis_hash: PolkadotHash,
		pub polkadot_vault_account_id: Option<PolkadotAccountId>,
		pub polkadot_runtime_version: RuntimeVersion,
		pub bitcoin_network: BitcoinNetwork,
	}

	/// Sets the genesis config
	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig {
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
			PolkadotRuntimeVersion::<T>::set(self.polkadot_runtime_version);
			PolkadotProxyAccountNonce::<T>::set(0);

			BitcoinAvailableUtxos::<T>::set(vec![]);
			BitcoinNetworkSelection::<T>::set(self.bitcoin_network);
		}
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

	pub fn add_bitcoin_utxo_to_list(
		amount: BtcAmount,
		utxo_id: UtxoId,
		script_pubkey: ScriptPubkey,
	) {
		let (salt, pubkey) = BitcoinActiveDepositAddressDetails::<T>::get(script_pubkey);

		BitcoinAvailableUtxos::<T>::append(Utxo {
			amount,
			id: utxo_id,
			deposit_address: DepositAddress::new(pubkey, salt),
		});
	}

	pub fn cleanup_bitcoin_deposit_address_details(script_pubkey: ScriptPubkey) {
		BitcoinActiveDepositAddressDetails::<T>::remove(script_pubkey);
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
		let BitcoinFeeInfo { fee_per_input_utxo, fee_per_output_utxo, min_fee_required_per_tx } =
			T::BitcoinFeeInfo::bitcoin_fee_info();
		match utxo_selection_type {
			UtxoSelectionType::SelectAllForRotation => {
				let available_utxos = BitcoinAvailableUtxos::<T>::take();
				(!available_utxos.is_empty()).then_some(available_utxos).map(|available_utxos| {
					(
						available_utxos.clone(),
						available_utxos.iter().map(|Utxo { amount, .. }| *amount).sum::<u64>() -
							(available_utxos.len() as u64) * fee_per_input_utxo -
							fee_per_output_utxo - min_fee_required_per_tx,
					)
				})
			},
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

	pub fn add_details_for_btc_deposit_script(
		script_pubkey: ScriptPubkey,
		salt: u32,
		pubkey: [u8; 32],
	) {
		BitcoinActiveDepositAddressDetails::<T>::insert(script_pubkey, (salt, pubkey));
	}
}
