// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(not(feature = "std"), no_std)]
#![feature(extract_if)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

use cf_chains::{
	btc::{
		api::{SelectedUtxosAndChangeAmount, UtxoSelectionType},
		deposit_address::DepositAddress,
		utxo_selection::{self, select_utxos_for_consolidation, select_utxos_from_pool},
		AggKey, Bitcoin, BtcAmount, Utxo, UtxoId, CHANGE_ADDRESS_SALT,
	},
	dot::{Polkadot, PolkadotAccountId, PolkadotHash, PolkadotIndex},
	eth::Address as EvmAddress,
	hub::{Assethub, OutputAccountId},
	sol::{
		api::{DurableNonceAndAccount, SolanaApi, SolanaEnvironment, SolanaGovCall},
		SolAddress, SolApiEnvironment, SolHash, Solana, NONCE_NUMBER_CRITICAL_NONCES,
	},
	Chain,
};
use cf_primitives::{
	chains::assets::{arb::Asset as ArbAsset, eth::Asset as EthAsset},
	BroadcastId, NetworkEnvironment, SemVer,
};
use cf_traits::{
	Broadcaster, CompatibleCfeVersions, GetBitcoinFeeInfo, KeyProvider, NetworkEnvironmentProvider,
	SafeMode, SolanaNonceWatch,
};
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

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(18);

const INITIAL_CONSOLIDATION_PARAMETERS: utxo_selection::ConsolidationParameters =
	utxo_selection::ConsolidationParameters {
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
	use cf_chains::{btc::Utxo, sol::api::DurableNonceAndAccount, Arbitrum};
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
		/// On new key witnessed handler for Arbitrum
		type ArbitrumVaultKeyWitnessedHandler: VaultKeyWitnessedHandler<Arbitrum>;
		/// On new key witnessed handler for Solana
		type SolanaVaultKeyWitnessedHandler: VaultKeyWitnessedHandler<Solana>;
		/// On new key witnessed handler for Assethub
		type AssethubVaultKeyWitnessedHandler: VaultKeyWitnessedHandler<Assethub>;

		/// For getting the current active AggKey. Used for rotating Utxos from previous vault.
		type BitcoinKeyProvider: KeyProvider<<Bitcoin as Chain>::ChainCrypto>;

		/// The runtime's safe mode is stored in this pallet.
		type RuntimeSafeMode: cf_traits::SafeMode + Member + Parameter + Default;

		/// Get Bitcoin Fee info from chain tracking
		type BitcoinFeeInfo: cf_traits::GetBitcoinFeeInfo;

		type SolanaNonceWatch: SolanaNonceWatch;

		/// Used to access the current Chainflip runtime's release version (distinct from the
		/// substrate RuntimeVersion)
		#[pallet::constant]
		type CurrentReleaseVersion: Get<SemVer>;

		type SolEnvironment: SolanaEnvironment;

		/// Solana broadcaster.
		type SolanaBroadcaster: Broadcaster<
			Solana,
			ApiCall = SolanaApi<Self::SolEnvironment>,
			Callback = RuntimeCallFor<Self>,
		>;

		/// Weight information
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Eth is not an Erc20 token, so its address can't be updated.
		EthAddressNotUpdateable,
		/// The nonce account is currently not being used or does not exist.
		NonceAccountNotBeingUsedOrDoesNotExist,
		/// The given UTXO parameters are invalid.
		InvalidUtxoParameters,
		/// Failed to build Solana Api call. See logs for more details
		FailedToBuildSolanaApiCall,
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
		StorageMap<_, Blake2_128Concat, EthAsset, EvmAddress>;

	#[pallet::storage]
	#[pallet::getter(fn state_chain_gateway_address)]
	/// The address of the ETH state chain gatweay contract
	pub type EthereumStateChainGatewayAddress<T> = StorageValue<_, EvmAddress, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn key_manager_address)]
	/// The address of the ETH key manager contract
	pub type EthereumKeyManagerAddress<T> = StorageValue<_, EvmAddress, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn eth_vault_address)]
	/// The address of the ETH vault contract
	pub type EthereumVaultAddress<T> = StorageValue<_, EvmAddress, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn eth_address_checker_address)]
	/// The address of the Address Checker contract on ETH
	pub type EthereumAddressCheckerAddress<T> = StorageValue<_, EvmAddress, ValueQuery>;

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
		StorageValue<_, utxo_selection::ConsolidationParameters, ValueQuery>;

	// ARBITRUM CHAIN RELATED ENVIRONMENT ITEMS
	#[pallet::storage]
	#[pallet::getter(fn supported_arb_assets)]
	/// Map of supported assets for ARB
	pub type ArbitrumSupportedAssets<T: Config> =
		StorageMap<_, Blake2_128Concat, ArbAsset, EvmAddress>;

	#[pallet::storage]
	#[pallet::getter(fn arb_key_manager_address)]
	/// The address of the ARB key manager contract
	pub type ArbitrumKeyManagerAddress<T> = StorageValue<_, EvmAddress, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn arb_vault_address)]
	/// The address of the ARB vault contract
	pub type ArbitrumVaultAddress<T> = StorageValue<_, EvmAddress, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn arb_address_checker_address)]
	/// The address of the Address Checker contract on Arbitrum.
	pub type ArbitrumAddressCheckerAddress<T> = StorageValue<_, EvmAddress, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn arbitrum_chain_id)]
	/// The ARB chain id
	pub type ArbitrumChainId<T> = StorageValue<_, cf_chains::evm::api::EvmChainId, ValueQuery>;

	#[pallet::storage]
	pub type ArbitrumSignatureNonce<T> = StorageValue<_, SignatureNonce, ValueQuery>;

	// SOLANA CHAIN RELATED ENVIRONMENT ITEMS
	#[pallet::storage]
	#[pallet::getter(fn solana_available_nonce_accounts)]
	pub type SolanaAvailableNonceAccounts<T> =
		StorageValue<_, Vec<DurableNonceAndAccount>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn solana_unavailable_nonce_accounts)]
	pub type SolanaUnavailableNonceAccounts<T> =
		StorageMap<_, Blake2_128Concat, SolAddress, SolHash>;

	#[pallet::storage]
	#[pallet::getter(fn sol_genesis_hash)]
	pub type SolanaGenesisHash<T> = StorageValue<_, SolHash, OptionQuery>;

	#[pallet::storage]
	#[pallet::getter(fn solana_api_environment)]
	pub type SolanaApiEnvironment<T> = StorageValue<_, SolApiEnvironment, ValueQuery>;

	// ASSETHUB CHAIN RELATED ENVIRONMENT ITEMS

	#[pallet::storage]
	#[pallet::getter(fn assethub_genesis_hash)]
	pub type AssethubGenesisHash<T> = StorageValue<_, PolkadotHash, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn assethub_vault_account)]
	/// The Assethub Vault Anonymous Account
	pub type AssethubVaultAccountId<T> = StorageValue<_, PolkadotAccountId, OptionQuery>;

	#[pallet::storage]
	/// Current Nonce of the current Assethub Proxy Account
	pub type AssethubProxyAccountNonce<T> = StorageValue<_, PolkadotIndex, ValueQuery>;

	#[pallet::storage]
	/// Current id used in "as_derivative" calls for CCM calls into Assethub
	pub type AssethubOutputAccountId<T> = StorageValue<_, OutputAccountId, ValueQuery>;

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
		AddedNewEthAsset(EthAsset, EvmAddress),
		/// The address of an supported ETH asset was updated
		UpdatedEthAsset(EthAsset, EvmAddress),
		/// A new supported ARB asset was added
		AddedNewArbAsset(ArbAsset, EvmAddress),
		/// The address of an supported ARB asset was updated
		UpdatedArbAsset(ArbAsset, EvmAddress),
		/// Polkadot Vault Account is successfully set
		PolkadotVaultAccountSet { polkadot_vault_account_id: PolkadotAccountId },
		/// The starting block number for the new Bitcoin vault was set
		BitcoinBlockNumberSetForVault { block_number: cf_chains::btc::BlockNumber },
		/// The Safe Mode settings for the chain has been updated
		RuntimeSafeModeUpdated { safe_mode: SafeModeUpdate<T> },
		/// Utxo consolidation parameters has been updated
		UtxoConsolidationParametersUpdated { params: utxo_selection::ConsolidationParameters },
		/// Arbitrum Initialized: contract addresses have been set, first key activated
		ArbitrumInitialized,
		/// Solana Initialized: contract addresses have been set, first key activated
		SolanaInitialized,
		/// Some unspendable Utxos are discarded from storage.
		StaleUtxosDiscarded { utxos: Vec<Utxo> },
		/// Solana durable nonce is updated to a new nonce for the corresponding nonce account.
		DurableNonceSetForAccount { nonce_account: SolAddress, durable_nonce: SolHash },
		/// An Governance transaction was dispatched to a Solana Program.
		SolanaGovCallDispatched { gov_call: SolanaGovCall, broadcast_id: BroadcastId },
		/// Assethub Vault Account is successfully set
		AssethubVaultAccountSet { assethub_vault_account_id: PolkadotAccountId },
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
		#[allow(unused_variables)]
		#[pallet::call_index(1)]
		// This weight is not strictly correct but since it's a governance call, weight is
		// irrelevant.
		#[pallet::weight(T::WeightInfo::witness_polkadot_vault_creation())]
		pub fn witness_polkadot_vault_creation(
			origin: OriginFor<T>,
			dot_pure_proxy_vault_key: PolkadotAccountId,
			tx_id: TxId,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			use cf_traits::VaultKeyWitnessedHandler;

			// Set Polkadot Pure Proxy Vault Account
			PolkadotVaultAccountId::<T>::put(dot_pure_proxy_vault_key);
			Self::deposit_event(Event::<T>::PolkadotVaultAccountSet {
				polkadot_vault_account_id: dot_pure_proxy_vault_key,
			});

			// Witness the agg_key rotation manually in the vaults pallet for polkadot
			T::PolkadotVaultKeyWitnessedHandler::on_first_key_activated(tx_id.block_number)
		}

		/// Manually witnesses the current Bitcoin block number to complete the pending vault
		/// rotation.
		#[allow(unused_variables)]
		#[pallet::call_index(2)]
		// This weight is not strictly correct but since it's a governance call, weight is
		// irrelevant.
		#[pallet::weight(T::WeightInfo::witness_current_bitcoin_block_number_for_key())]
		pub fn witness_current_bitcoin_block_number_for_key(
			origin: OriginFor<T>,
			block_number: cf_chains::btc::BlockNumber,
			new_public_key: cf_chains::btc::AggKey,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			use cf_traits::VaultKeyWitnessedHandler;

			// Witness the agg_key rotation manually in the vaults pallet for bitcoin
			T::BitcoinVaultKeyWitnessedHandler::on_first_key_activated(block_number)?;

			Self::deposit_event(Event::<T>::BitcoinBlockNumberSetForVault { block_number });

			Ok(())
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
			params: utxo_selection::ConsolidationParameters,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			ensure!(params.are_valid(), Error::<T>::InvalidUtxoParameters);

			ConsolidationParameters::<T>::set(params);

			Self::deposit_event(Event::<T>::UtxoConsolidationParametersUpdated { params });

			Ok(())
		}

		/// Manually witnesses the current Arbitrum block number to complete the pending vault
		/// rotation.
		#[pallet::call_index(5)]
		// This weight is not strictly correct but since it's a governance call, weight is
		// irrelevant.
		#[pallet::weight(T::WeightInfo::witness_initialize_arbitrum_vault())]
		pub fn witness_initialize_arbitrum_vault(
			origin: OriginFor<T>,
			block_number: u64,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			use cf_traits::VaultKeyWitnessedHandler;

			// Witness the agg_key rotation manually in the vaults pallet for Arbitrum
			T::ArbitrumVaultKeyWitnessedHandler::on_first_key_activated(block_number)?;

			Self::deposit_event(Event::<T>::ArbitrumInitialized);

			Ok(())
		}

		#[pallet::call_index(6)]
		// This weight is not strictly correct but since it's a governance call, weight is
		// irrelevant.
		#[pallet::weight(T::WeightInfo::witness_initialize_solana_vault())]
		pub fn witness_initialize_solana_vault(
			origin: OriginFor<T>,
			block_number: u64,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			use cf_traits::VaultKeyWitnessedHandler;

			// Witness the agg_key rotation manually in the vaults pallet for Solana
			T::SolanaVaultKeyWitnessedHandler::on_first_key_activated(block_number)?;

			Self::deposit_event(Event::<T>::SolanaInitialized);

			Ok(())
		}

		/// Allows Governance to recover a used Nonce.
		/// If a Hash is supplied as well, update the associated Durable Hash as well.
		/// Requires Governance Origin.
		#[pallet::call_index(7)]
		#[pallet::weight(T::WeightInfo::force_recover_sol_nonce())]
		pub fn force_recover_sol_nonce(
			origin: OriginFor<T>,
			nonce_account: SolAddress,
			durable_nonce: Option<SolHash>,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			let new_hash =
				// If Nonce account is currently Unavailable - reset it as Available again.
				if let Some(current_hash) = SolanaUnavailableNonceAccounts::<T>::take(nonce_account) {
					let new_hash = durable_nonce.unwrap_or(current_hash);
					SolanaAvailableNonceAccounts::<T>::append((nonce_account, new_hash));
					Ok(new_hash)
				} else if let Some(new_hash) = durable_nonce {
					// If the Nonce account is currently Available, update its Hash (therefore the Hash must be passed in)
					SolanaAvailableNonceAccounts::<T>::try_mutate(|durable_nonces|{
						durable_nonces
							.iter()
							.position(|(account, _)|*account == nonce_account)
							.map(|idx| {
								durable_nonces[idx] = (nonce_account, new_hash);
								new_hash
							})
							.ok_or::<DispatchError>(Error::<T>::NonceAccountNotBeingUsedOrDoesNotExist.into())
					})
				} else {
					// The Nonce account currently isn't being used, or no Hash is given when it's required.
					Err(Error::<T>::NonceAccountNotBeingUsedOrDoesNotExist.into())
				}?;

			Self::deposit_event(Event::<T>::DurableNonceSetForAccount {
				nonce_account,
				durable_nonce: new_hash,
			});

			Ok(())
		}

		/// **READ WARNINGS BEFORE USING THIS**
		///
		/// Allows Governance to dispatch calls to the Solana Contracts.
		///
		/// Note this will only work as long as the Solana GovKey is the current AggKey, which might
		/// change in the future.
		///
		/// Requires Governance Origin. This action is allowed to consume any nonce account because
		/// it's a high priority action. Therefore, **DO NOT** execute this governance function
		/// around a rotation as it could consume the nonce saved for rotations.
		#[pallet::call_index(8)]
		#[pallet::weight(T::WeightInfo::dispatch_solana_gov_call())]
		pub fn dispatch_solana_gov_call(
			origin: OriginFor<T>,
			gov_call: SolanaGovCall,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			let (broadcast_id, _) =
				T::SolanaBroadcaster::threshold_sign_and_broadcast(gov_call.to_api_call().map_err(|e| {
				// If we fail here, most likely some Solana Environment variables were not set.
				log::error!("Failed to build Solana Api call to update Solana Vault Swap settings. Error: {:?}", e);
				Error::<T>::FailedToBuildSolanaApiCall
			})?);

			Self::deposit_event(Event::<T>::SolanaGovCallDispatched { gov_call, broadcast_id });

			Ok(())
		}

		/// /// Manually initiates Assethub vault key rotation completion steps so Epoch rotation
		/// can be continued and sets the Assethub Pure Proxy Vault in environment pallet. The
		/// extrinsic takes in the hub_pure_proxy_vault_key, which is obtained from the Assethub
		/// blockchain as a result of creating an assethub vault which is done by executing the
		/// extrinsic create_assethub_vault(), hub_witnessed_aggkey, the aggkey which initiated
		/// the assethub creation transaction and the tx hash and block number of the Assethub
		/// block the vault creation transaction was witnessed in. This extrinsic should complete
		/// the Assethub initiation process and the vault should rotate successfully.
		#[allow(unused_variables)]
		#[pallet::call_index(9)]
		// This weight is not strictly correct but since it's a governance call, weight is
		// irrelevant.
		#[pallet::weight(T::WeightInfo::witness_assethub_vault_creation())]
		pub fn witness_assethub_vault_creation(
			origin: OriginFor<T>,
			hub_pure_proxy_vault_key: PolkadotAccountId,
			tx_id: TxId,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			use cf_traits::VaultKeyWitnessedHandler;

			// Set Assethub Pure Proxy Vault Account
			AssethubVaultAccountId::<T>::put(hub_pure_proxy_vault_key);
			Self::deposit_event(Event::<T>::AssethubVaultAccountSet {
				assethub_vault_account_id: hub_pure_proxy_vault_key,
			});

			// Witness the agg_key rotation manually in the vaults pallet for assethub
			T::AssethubVaultKeyWitnessedHandler::on_first_key_activated(tx_id.block_number)
		}
	}

	#[pallet::genesis_config]
	#[derive(DefaultNoBound)]
	pub struct GenesisConfig<T> {
		pub flip_token_address: EvmAddress,
		pub eth_usdc_address: EvmAddress,
		pub eth_usdt_address: EvmAddress,
		pub state_chain_gateway_address: EvmAddress,
		pub eth_key_manager_address: EvmAddress,
		pub eth_vault_address: EvmAddress,
		pub eth_address_checker_address: EvmAddress,
		pub ethereum_chain_id: u64,
		pub polkadot_genesis_hash: PolkadotHash,
		pub polkadot_vault_account_id: Option<PolkadotAccountId>,
		pub arb_usdc_address: EvmAddress,
		pub arb_key_manager_address: EvmAddress,
		pub arb_vault_address: EvmAddress,
		pub arb_address_checker_address: EvmAddress,
		pub arbitrum_chain_id: u64,
		pub network_environment: NetworkEnvironment,
		pub sol_genesis_hash: Option<SolHash>,
		pub sol_api_env: SolApiEnvironment,
		pub sol_durable_nonces_and_accounts: Vec<DurableNonceAndAccount>,
		pub assethub_genesis_hash: PolkadotHash,
		pub assethub_vault_account_id: Option<PolkadotAccountId>,
		pub _config: PhantomData<T>,
	}

	/// Sets the genesis config
	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			EthereumStateChainGatewayAddress::<T>::set(self.state_chain_gateway_address);
			EthereumKeyManagerAddress::<T>::set(self.eth_key_manager_address);
			EthereumVaultAddress::<T>::set(self.eth_vault_address);
			EthereumAddressCheckerAddress::<T>::set(self.eth_address_checker_address);

			EthereumChainId::<T>::set(self.ethereum_chain_id);
			EthereumSupportedAssets::<T>::insert(EthAsset::Flip, self.flip_token_address);
			EthereumSupportedAssets::<T>::insert(EthAsset::Usdc, self.eth_usdc_address);
			EthereumSupportedAssets::<T>::insert(EthAsset::Usdt, self.eth_usdt_address);

			PolkadotGenesisHash::<T>::set(self.polkadot_genesis_hash);
			PolkadotVaultAccountId::<T>::set(self.polkadot_vault_account_id);
			PolkadotProxyAccountNonce::<T>::set(0);

			BitcoinAvailableUtxos::<T>::set(vec![]);
			ConsolidationParameters::<T>::set(INITIAL_CONSOLIDATION_PARAMETERS);

			ArbitrumKeyManagerAddress::<T>::set(self.arb_key_manager_address);
			ArbitrumVaultAddress::<T>::set(self.arb_vault_address);
			ArbitrumChainId::<T>::set(self.arbitrum_chain_id);
			ArbitrumSupportedAssets::<T>::insert(ArbAsset::ArbUsdc, self.arb_usdc_address);
			ArbitrumAddressCheckerAddress::<T>::set(self.arb_address_checker_address);

			SolanaGenesisHash::<T>::set(self.sol_genesis_hash);
			SolanaApiEnvironment::<T>::set(self.sol_api_env.clone());
			SolanaAvailableNonceAccounts::<T>::set(self.sol_durable_nonces_and_accounts.clone());

			AssethubGenesisHash::<T>::set(self.assethub_genesis_hash);
			AssethubVaultAccountId::<T>::set(self.assethub_vault_account_id);
			AssethubProxyAccountNonce::<T>::set(0);

			AssethubOutputAccountId::<T>::set(1);

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

	pub fn next_arbitrum_signature_nonce() -> SignatureNonce {
		ArbitrumSignatureNonce::<T>::mutate(|nonce| {
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

	pub fn add_bitcoin_utxo_to_list(utxo: Utxo) {
		BitcoinAvailableUtxos::<T>::append(utxo);
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
		let min_fee_required_per_tx = bitcoin_fee_info.min_fee_required_per_tx();
		let fee_per_output_utxo = bitcoin_fee_info.fee_per_output_utxo();

		fn filter_stale_utxos<T: Config>(available_utxos: &mut Vec<Utxo>, aggkey: &AggKey) {
			if let Some(previous) = aggkey.previous {
				let stale = available_utxos
					.extract_if(.., |utxo| {
						utxo.deposit_address.pubkey_x != aggkey.current &&
							utxo.deposit_address.pubkey_x != previous
					})
					.collect::<Vec<_>>();
				if !stale.is_empty() {
					log::warn!("Stale utxos detected: {:?}", stale);
					Pallet::<T>::deposit_event(Event::<T>::StaleUtxosDiscarded { utxos: stale });
				}
			}
		}

		match utxo_selection_type {
			UtxoSelectionType::SelectForConsolidation =>
				BitcoinAvailableUtxos::<T>::mutate(|available_utxos| {
					if let Some(cf_traits::EpochKey {
						key: aggkey @ AggKey { previous, .. }, ..
					}) = T::BitcoinKeyProvider::active_epoch_key()
					{
						filter_stale_utxos::<T>(available_utxos, &aggkey);

						let selected_utxo = select_utxos_for_consolidation(
							previous,
							available_utxos,
							&bitcoin_fee_info,
							Self::consolidation_parameters(),
						);

						Self::consolidation_transaction_change_amount(
							&selected_utxo[..],
							&bitcoin_fee_info,
						)
						.map(|change_amount| (selected_utxo, change_amount))
					} else {
						None
					}
				}),
			UtxoSelectionType::Some { output_amount, number_of_outputs } =>
				BitcoinAvailableUtxos::<T>::try_mutate(|available_utxos| {
					if let Some(cf_traits::EpochKey { key: aggkey, .. }) =
						T::BitcoinKeyProvider::active_epoch_key()
					{
						filter_stale_utxos::<T>(available_utxos, &aggkey);
					}
					select_utxos_from_pool(
						available_utxos,
						&bitcoin_fee_info,
						output_amount +
							number_of_outputs * fee_per_output_utxo +
							min_fee_required_per_tx,
						Some(Self::consolidation_parameters().consolidation_threshold),
					)
					.map_err(|error| {
						log::warn!(
							"Unable to select desired amount from available utxos. Error: {:?}",
							error
						);
						error
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

	fn consolidation_transaction_change_amount(
		spendable_utxos: &[Utxo],
		fee_info: &cf_chains::btc::BitcoinFeeInfo,
	) -> Option<BtcAmount> {
		if spendable_utxos.is_empty() {
			return None
		}

		spendable_utxos.iter().map(|utxo| utxo.amount).sum::<BtcAmount>().checked_sub(
			fee_info.fee_per_input_utxo() * spendable_utxos.len() as BtcAmount +
				fee_info.min_fee_required_per_tx() +
				fee_info.fee_per_output_utxo(),
		)
	}

	pub fn get_sol_nonce_and_account() -> Option<DurableNonceAndAccount> {
		let nonce_and_account = SolanaAvailableNonceAccounts::<T>::mutate(|nonce_and_accounts| {
			nonce_and_accounts.pop()
		});
		nonce_and_account.map(|(account, nonce)| {
			SolanaUnavailableNonceAccounts::<T>::insert(account, nonce);
			if let Err(err) = T::SolanaNonceWatch::watch_for_nonce_change(account, nonce) {
				log::error!("Error initiating watch for nonce change: {:?}", err);
			}
			(account, nonce)
		})
	}

	/// IMPORTANT: This fn is used to recover an un-used DurableNonce so it's available again.
	/// ONLY use this if this nonce is un-used.
	pub fn recover_sol_durable_nonce(nonce_account: SolAddress) {
		if let Some(hash) = SolanaUnavailableNonceAccounts::<T>::take(nonce_account) {
			SolanaAvailableNonceAccounts::<T>::append((nonce_account, hash));
		}
	}

	pub fn get_all_sol_nonce_accounts() -> Vec<DurableNonceAndAccount> {
		let mut nonce_accounts = SolanaAvailableNonceAccounts::<T>::get();
		nonce_accounts.extend(&mut SolanaUnavailableNonceAccounts::<T>::iter());
		nonce_accounts
	}

	// Get the number of available nonce accounts. We want to leave a number of available nonces
	// at all time for critical operations such as vault rotations or governance actions.
	pub fn get_number_of_available_sol_nonce_accounts(critical: bool) -> usize {
		let number_nonces = SolanaAvailableNonceAccounts::<T>::decode_len().unwrap_or(0);
		if !critical {
			number_nonces.saturating_sub(NONCE_NUMBER_CRITICAL_NONCES)
		} else {
			number_nonces
		}
	}

	pub fn update_sol_nonce(nonce_account: SolAddress, durable_nonce: SolHash) {
		if let Some(_nonce) = SolanaUnavailableNonceAccounts::<T>::take(nonce_account) {
			SolanaAvailableNonceAccounts::<T>::append((nonce_account, durable_nonce));
			Self::deposit_event(Event::<T>::DurableNonceSetForAccount {
				nonce_account,
				durable_nonce,
			});
		} else {
			log::error!("Nonce account {nonce_account} not found in unavailable nonce accounts");
		}
	}

	pub fn next_assethub_proxy_account_nonce(reset_nonce: bool) -> PolkadotIndex {
		AssethubProxyAccountNonce::<T>::mutate(|nonce| {
			let current_nonce = *nonce;

			if reset_nonce {
				*nonce = 0;
			} else {
				*nonce += 1;
			}
			current_nonce
		})
	}

	pub fn next_assethub_output_account_id() -> OutputAccountId {
		AssethubOutputAccountId::<T>::mutate(|id| {
			let current_id = *id;
			*id += 1;
			current_id
		})
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
