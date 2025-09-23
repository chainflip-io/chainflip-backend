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
	dot::{api::PolkadotApi, Polkadot, PolkadotAccountId, PolkadotHash, PolkadotIndex},
	eth::Address as EvmAddress,
	evm::Signature as EthereumSignature,
	hub::{Assethub, OutputAccountId},
	sol::{
		api::{DurableNonceAndAccount, SolanaApi, SolanaEnvironment, SolanaGovCall},
		SolAddress, SolApiEnvironment, SolHash, SolSignature, Solana, NONCE_NUMBER_CRITICAL_NONCES,
	},
	Chain, ReplayProtectionProvider,
};
use cf_primitives::{
	chains::assets::{arb::Asset as ArbAsset, eth::Asset as EthAsset},
	BlockNumber, BroadcastId, ChainflipNetwork, NetworkEnvironment, SemVer,
};
use cf_traits::{
	Broadcaster, CompatibleCfeVersions, GetBitcoinFeeInfo, KeyProvider,
	NetworkEnvironmentProvider, SafeMode, SolanaNonceWatch,
};
use codec::{Decode, Encode};
use frame_support::{
	dispatch::{DispatchResult, GetDispatchInfo},
	pallet_prelude::{InvalidTransaction, *},
	sp_runtime::{traits::Get, AccountId32, DispatchError, TransactionOutcome},
	storage::with_transaction,
	traits::{StorageVersion, UnfilteredDispatchable},
	unsigned::{TransactionValidity, ValidateUnsigned},
};
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_std::{vec, vec::Vec};

mod benchmarking;
mod mock;
mod tests;

pub mod weights;
pub use weights::WeightInfo;
pub mod migrations;

const ETHEREUM_SIGN_MESSAGE_PREFIX: &str = "\x19Ethereum Signed Message:\n";
const SOLANA_OFFCHAIN_PREFIX: &[u8] = b"\xffsolana offchain";

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(19);

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
	use cf_chains::{
		btc::Utxo, dot::api::PolkadotEnvironment, hub::api::AssethubEnvironment,
		sol::api::DurableNonceAndAccount, Arbitrum,
	};
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

		/// Only required for 1.11 (deprecation of polkadot and migration to assethub).
		type DotEnvironment: PolkadotEnvironment + ReplayProtectionProvider<Polkadot>;

		/// Only required for 1.11 (deprecation of polkadot and migration to assethub).
		type HubEnvironment: AssethubEnvironment;

		/// Only required for 1.11 (deprecation of polkadot and migration to assethub).
		type PolkadotBroadcaster: Broadcaster<
			Polkadot,
			ApiCall = PolkadotApi<Self::DotEnvironment>,
			Callback = RuntimeCallFor<Self>,
		>;

		type RuntimeOrigin: From<frame_system::RawOrigin<<Self as frame_system::Config>::AccountId>>;

		/// The overarching call type.
		type RuntimeCall: Member
			+ Parameter
			+ UnfilteredDispatchable<RuntimeOrigin = <Self as Config>::RuntimeOrigin>
			+ From<frame_system::Call<Self>>
			+ From<Call<Self>>
			+ GetDispatchInfo;

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
		/// Payload has expired
		PayloadExpired,
		// Signer cannot be decoded
		FailedToDecodeSigner,
		// Signature failed to be verified
		InvalidSignature,
		// Nonce missmatch
		InvalidNonce,
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
	#[pallet::getter(fn eth_sc_utils_address)]
	/// The address of the Sc Utils contract on ETH
	pub type EthereumScUtilsAddress<T> = StorageValue<_, EvmAddress, ValueQuery>;

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

	#[pallet::storage]
	#[pallet::getter(fn chainflip_network)]
	/// Current Chainflip's network name
	pub type ChainflipNetworkName<T> = StorageValue<_, ChainflipNetwork, ValueQuery>;

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
		UserActionSubmitted {
			signer_account_id: T::AccountId,
			serialized_call: Vec<u8>,
			dispatch_result: DispatchResultWithPostInfo,
		},
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
				SafeModeUpdate::CodeGreen => SafeMode::code_green(),
				SafeModeUpdate::CodeRed => SafeMode::code_red(),
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

		/// Sign polkadot extrinsic to teleport vault polkadot balance to assethub
		/// Only required during the upgrade to 1.11, delete afterwards.
		#[pallet::call_index(10)]
		#[pallet::weight(Weight::zero())]
		pub fn dispatch_polkadot_vault_migration_to_assethub(
			origin: OriginFor<T>,
			amount: cf_chains::dot::PolkadotBalance,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			let polkadot_vault = T::DotEnvironment::try_vault_account()
				.ok_or(DispatchError::Other("Could not get polkadot vault"))?;

			let assethub_vault = T::HubEnvironment::try_vault_account()
				.ok_or(DispatchError::Other("Could not get assethub vault"))?;

			let replay_protection = T::DotEnvironment::replay_protection(false);

			// create polkadot extrinsic for teleporting vault to our
			let runtime_call = cf_chains::dot::api::migrate_polkadot::extrinsic_builder(
				amount,
				replay_protection,
				polkadot_vault,
				assethub_vault,
			);

			let (_broadcast_id, _) = T::PolkadotBroadcaster::threshold_sign_and_broadcast(
				cf_chains::dot::api::PolkadotApi::MigrateToAssethub(runtime_call),
			);

			Ok(())
		}

		#[pallet::call_index(11)]
		#[pallet::weight(T::WeightInfo::submit_user_signed_payload())]
		pub fn submit_user_signed_payload(
			origin: OriginFor<T>,
			call: sp_std::boxed::Box<<T as Config>::RuntimeCall>,
			_transaction_metadata: TransactionMetadata,
			user_signature_data: UserSignatureData,
		) -> DispatchResult {
			use frame_system::ensure_none;

			// This is now an unsigned extrinsic - validation happens in ValidateUnsigned
			ensure_none(origin)?;

			// Extract signer account ID based on signature type - validation already done in ValidateUnsigned
			let signer_account_id: T::AccountId = user_signature_data
				.signer_account_id::<T>()
				.map_err(|_| Error::<T>::FailedToDecodeSigner)?;

			// Increment the account nonce to prevent replay attacks
			frame_system::Pallet::<T>::inc_account_nonce(&signer_account_id);

			let dispatch_result =
				Self::dispatch_user_call(*call.clone(), signer_account_id.clone());

			Self::deposit_event(Event::<T>::UserActionSubmitted {
				signer_account_id,
				serialized_call: call.encode(),
				dispatch_result,
			});

			Ok(())
		}
	}

	#[pallet::validate_unsigned]
	impl<T: Config> ValidateUnsigned for Pallet<T> {
		type Call = Call<T>;

		fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			if let Call::submit_user_signed_payload { call, transaction_metadata, user_signature_data } = call {
				use cf_chains::{
					evm::EvmCrypto,
					sol::SolanaCrypto,
					ChainCrypto,
				};

				// Check if payload hasn't expired
				if frame_system::Pallet::<T>::block_number() >= transaction_metadata.expiry_block.into() {
					return InvalidTransaction::Stale.into();
				}

				let chanflip_network_name = Self::chainflip_network();
				let serialized_call: Vec<u8> = call.encode();


				let valid_signature = match user_signature_data {
					UserSignatureData::Solana { signature, signer, sig_type } => {
						let signed_payload = match sig_type {
							SolSigType::Domain => {
								let concat_data = [
									serialized_call.clone(),
									chanflip_network_name.as_str().encode(),
									transaction_metadata.encode(),
								]
								.concat();
								[SOLANA_OFFCHAIN_PREFIX, concat_data.as_slice()].concat()
							},
						};
						SolanaCrypto::verify_signature(signer, &signed_payload, signature)
					},
					UserSignatureData::Ethereum { signature, signer, sig_type } => {
						let signed_payload = match sig_type {
							EthSigType::Domain => {
								let concat_data = [
									serialized_call.clone(),
									chanflip_network_name.as_str().encode(),
									transaction_metadata.encode(),
								]
								.concat();
								let prefix = scale_info::prelude::format!(
									"{}{}",
									ETHEREUM_SIGN_MESSAGE_PREFIX,
									concat_data.len()
								);
								let prefix_bytes = prefix.as_bytes();
								[prefix_bytes, &concat_data].concat()
							},
							EthSigType::Eip712 => {
								Self::build_eip_712_payload(
									*call.clone(),
									transaction_metadata.clone(),
									chanflip_network_name,
									*signer,
								)
							},
						};
						EvmCrypto::verify_signature(signer, &signed_payload, signature)
					},
				};

				if !valid_signature {
					return InvalidTransaction::BadProof.into();
				}

				// Extract signer account ID
				let signer_account_id = match user_signature_data.signer_account_id::<T>() {
					Ok(account_id) => account_id,
					Err(_) => return InvalidTransaction::BadSigner.into(),
				};

				// Check nonce
				let signer_current_nonce = frame_system::Pallet::<T>::account_nonce(&signer_account_id);
				if signer_current_nonce != transaction_metadata.nonce.into() {
					return InvalidTransaction::Stale.into();
				}


				// TODO: We could also use Self::name(), depends on the final implementation/structure of this.
				let unique_id = (signer_account_id, transaction_metadata.nonce);
				ValidTransaction::with_tag_prefix("user-signed-payload")
					.and_provides(unique_id)
					.build()
			} else {
				InvalidTransaction::Call.into()
			}
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
		pub eth_sc_utils_address: EvmAddress,
		pub ethereum_chain_id: u64,
		pub polkadot_genesis_hash: PolkadotHash,
		pub polkadot_vault_account_id: Option<PolkadotAccountId>,
		pub arb_usdc_address: EvmAddress,
		pub arb_key_manager_address: EvmAddress,
		pub arb_vault_address: EvmAddress,
		pub arb_address_checker_address: EvmAddress,
		pub arbitrum_chain_id: u64,
		pub network_environment: NetworkEnvironment,
		pub chainflip_network: ChainflipNetwork,
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
			EthereumScUtilsAddress::<T>::set(self.eth_sc_utils_address);

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
			ChainflipNetworkName::<T>::set(self.chainflip_network);

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

	/// Dispatches a call from the user account, with transactional semantics, ie. if the call
	/// dispatch returns `Err`, rolls back any storage updates.
	fn dispatch_user_call(
		call: <T as Config>::RuntimeCall,
		user_account: T::AccountId,
	) -> DispatchResultWithPostInfo {
		with_transaction(move || {
			match call.dispatch_bypass_filter(frame_system::RawOrigin::Signed(user_account).into())
			{
				r @ Ok(_) => TransactionOutcome::Commit(r),
				r @ Err(_) => TransactionOutcome::Rollback(r),
			}
		})
	}

	/// `signer is not technically necessary but is added as part of the metadata so
	/// it is displayed separately to the user in the wallet
	fn build_eip_712_payload(
		_call: <T as Config>::RuntimeCall,
		_transaction_metadata: TransactionMetadata,
		_chain_name: ChainflipNetwork,
		_signer: EvmAddress,
	) -> Vec<u8> {
		todo!("implement eip-712");
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

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct TransactionMetadata {
	nonce: u32,
	expiry_block: BlockNumber,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum EthSigType {
	Domain, // personal_sign
	Eip712,
}
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum SolSigType {
	Domain, /* Using `b"\xffsolana offchain" as per Anza specifications,
	         * even if we are not using the proposal. Phantom might use
	         * a different standard though..
	         * References
	         * https://docs.anza.xyz/proposals/off-chain-message-signing
	         * And/or phantom off-chain signing:
	         * https://github.com/phantom/sign-in-with-solana */
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum UserSignatureData {
	Solana { signature: SolSignature, signer: SolAddress, sig_type: SolSigType },
	Ethereum { signature: EthereumSignature, signer: EvmAddress, sig_type: EthSigType },
}

impl UserSignatureData {
	/// Extract the signer account ID as T::AccountId from the signature data
	pub fn signer_account_id<T: Config>(&self) -> Result<T::AccountId, codec::Error> {
		use cf_chains::evm::ToAccountId32;
		
		let account_id_32 = match self {
			UserSignatureData::Solana { signer, .. } => AccountId32::new((*signer).into()),
			UserSignatureData::Ethereum { signer, .. } => signer.into_account_id_32(),
		};
		
		T::AccountId::decode(&mut account_id_32.encode().as_slice())
	}
}
