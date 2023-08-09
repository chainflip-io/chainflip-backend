//! Configuration, utilities and helpers for the Chainflip runtime.
pub mod address_derivation;
pub mod all_vaults_rotator;
mod backup_node_rewards;
pub mod chain_instances;
pub mod decompose_recompose;
pub mod epoch_transition;
mod missed_authorship_slots;
mod offences;
mod signer_nomination;
use crate::{
	AccountId, AccountRoles, Authorship, BitcoinChainTracking, BitcoinIngressEgress, BitcoinVault,
	BlockNumber, Emissions, Environment, EthereumBroadcaster, EthereumChainTracking,
	EthereumIngressEgress, Flip, FlipBalance, PolkadotBroadcaster, PolkadotChainTracking,
	PolkadotIngressEgress, Runtime, RuntimeCall, System, Validator,
};
use backup_node_rewards::calculate_backup_rewards;
use cf_chains::{
	address::{
		to_encoded_address, try_from_encoded_address, AddressConverter, EncodedAddress,
		ForeignChainAddress,
	},
	btc::{
		api::{BitcoinApi, SelectedUtxosAndChangeAmount, UtxoSelectionType},
		Bitcoin, BitcoinFeeInfo, BitcoinTransactionData, ScriptPubkey, UtxoId,
	},
	dot::{
		api::PolkadotApi, Polkadot, PolkadotAccountId, PolkadotReplayProtection,
		PolkadotTransactionData, RuntimeVersion,
	},
	eth::{
		self,
		api::{EthEnvironmentProvider, EthereumApi, EthereumContract, EthereumReplayProtection},
		deposit_address::ETHEREUM_ETH_ADDRESS,
		Ethereum,
	},
	AnyChain, ApiCall, CcmChannelMetadata, CcmDepositMetadata, Chain, ChainAbi, ChainCrypto,
	ChainEnvironment, ForeignChain, ReplayProtectionProvider, SetCommKeyWithAggKey,
	SetGovKeyWithAggKey, TransactionBuilder,
};
use cf_primitives::{chains::assets, AccountRole, Asset, BasisPoints, ChannelId, EgressId};
use cf_traits::{
	impl_runtime_safe_mode, AccountRoleRegistry, BlockEmissions, BroadcastAnyChainGovKey,
	Broadcaster, Chainflip, CommKeyBroadcaster, DepositApi, DepositHandler, EgressApi, EpochInfo,
	Heartbeat, Issuance, KeyProvider, OnBroadcastReady, QualifyNode, RewardsDistribution,
	RuntimeUpgrade, VaultTransitionHandler,
};
use codec::{Decode, Encode};
use frame_support::{
	dispatch::{DispatchError, DispatchErrorWithPostInfo, PostDispatchInfo},
	traits::Get,
};
pub use missed_authorship_slots::MissedAuraSlots;
pub use offences::*;
use scale_info::TypeInfo;
pub use signer_nomination::RandomSignerNomination;
use sp_core::U256;
use sp_runtime::traits::{BlockNumberProvider, UniqueSaturatedFrom, UniqueSaturatedInto};
use sp_std::prelude::*;

impl Chainflip for Runtime {
	type RuntimeCall = RuntimeCall;
	type Amount = FlipBalance;
	type ValidatorId = <Self as frame_system::Config>::AccountId;
	type EnsureWitnessed = pallet_cf_witnesser::EnsureWitnessed;
	type EnsureWitnessedAtCurrentEpoch = pallet_cf_witnesser::EnsureWitnessedAtCurrentEpoch;
	type EnsureGovernance = pallet_cf_governance::EnsureGovernance;
	type EpochInfo = Validator;
	type AccountRoleRegistry = AccountRoles;
	type FundingInfo = Flip;
}

impl_runtime_safe_mode! {
	RuntimeSafeMode,
	pallet_cf_environment::RuntimeSafeMode<Runtime>,
	emissions: pallet_cf_emissions::PalletSafeMode,
	funding: pallet_cf_funding::PalletSafeMode,
	swapping: pallet_cf_swapping::PalletSafeMode,
	liquidity_provider: pallet_cf_lp::PalletSafeMode,
	validator: pallet_cf_validator::PalletSafeMode,
	pools: pallet_cf_pools::PalletSafeMode,
	reputation: pallet_cf_reputation::PalletSafeMode,
	vault: pallet_cf_vaults::PalletSafeMode,
}
struct BackupNodeEmissions;

impl RewardsDistribution for BackupNodeEmissions {
	type Balance = FlipBalance;
	type Issuance = pallet_cf_flip::FlipIssuance<Runtime>;

	fn distribute() {
		let backup_nodes =
			Validator::highest_funded_qualified_backup_node_bids().collect::<Vec<_>>();
		if backup_nodes.is_empty() {
			return
		}

		// Distribute rewards one by one
		// N.B. This could be more optimal
		for (validator_id, reward) in calculate_backup_rewards(
			backup_nodes,
			Validator::bond(),
			<<Runtime as pallet_cf_reputation::Config>::HeartbeatBlockInterval as Get<
				BlockNumber,
			>>::get()
			.unique_saturated_into(),
			Emissions::backup_node_emission_per_block(),
			Emissions::current_authority_emission_per_block(),
			Self::Balance::unique_saturated_from(Validator::current_authority_count()),
		) {
			Flip::settle(&validator_id, Self::Issuance::mint(reward).into());
		}
	}
}

pub struct ChainflipHeartbeat;

impl Heartbeat for ChainflipHeartbeat {
	type ValidatorId = AccountId;
	type BlockNumber = BlockNumber;

	fn on_heartbeat_interval() {
		<Emissions as BlockEmissions>::calculate_block_emissions();
		BackupNodeEmissions::distribute();
	}
}

/// Checks if the caller can execute free transactions
pub struct WaivedFees;

impl cf_traits::WaivedFees for WaivedFees {
	type AccountId = AccountId;
	type RuntimeCall = RuntimeCall;

	fn should_waive_fees(call: &Self::RuntimeCall, caller: &Self::AccountId) -> bool {
		if matches!(call, RuntimeCall::Governance(_)) {
			return super::Governance::members().contains(caller)
		}
		false
	}
}

pub struct EthTransactionBuilder;

impl TransactionBuilder<Ethereum, EthereumApi<EthEnvironment>> for EthTransactionBuilder {
	fn build_transaction(
		signed_call: &EthereumApi<EthEnvironment>,
	) -> <Ethereum as ChainAbi>::Transaction {
		eth::Transaction {
			chain_id: signed_call.replay_protection().chain_id,
			contract: signed_call.replay_protection().contract_address,
			data: signed_call.chain_encoded(),
			..Default::default()
		}
	}

	fn refresh_unsigned_data(unsigned_tx: &mut <Ethereum as ChainAbi>::Transaction) {
		let tracked_data = EthereumChainTracking::chain_state().unwrap().tracked_data;
		// double the last block's base fee. This way we know it'll be selectable for at least 6
		// blocks (12.5% increase on each block)
		let max_fee_per_gas = tracked_data
			.base_fee
			.saturating_mul(2)
			.saturating_add(tracked_data.priority_fee);
		unsigned_tx.max_fee_per_gas = Some(U256::from(max_fee_per_gas));
		unsigned_tx.max_priority_fee_per_gas = Some(U256::from(tracked_data.priority_fee));
	}

	fn is_valid_for_rebroadcast(
		call: &EthereumApi<EthEnvironment>,
		_payload: &<Ethereum as ChainCrypto>::Payload,
		current_key: &<Ethereum as ChainCrypto>::AggKey,
		signature: &<Ethereum as ChainCrypto>::ThresholdSignature,
	) -> bool {
		// Check if signature is valid
		<Ethereum as ChainCrypto>::verify_threshold_signature(
			current_key,
			&call.threshold_signature_payload(),
			signature,
		)
	}
}

pub struct DotTransactionBuilder;
impl TransactionBuilder<Polkadot, PolkadotApi<DotEnvironment>> for DotTransactionBuilder {
	fn build_transaction(
		signed_call: &PolkadotApi<DotEnvironment>,
	) -> <Polkadot as ChainAbi>::Transaction {
		PolkadotTransactionData { encoded_extrinsic: signed_call.chain_encoded() }
	}

	fn refresh_unsigned_data(_unsigned_tx: &mut <Polkadot as ChainAbi>::Transaction) {
		// TODO: For now this is a noop until we actually have dot chain tracking
	}

	fn is_valid_for_rebroadcast(
		call: &PolkadotApi<DotEnvironment>,
		payload: &<Polkadot as ChainCrypto>::Payload,
		current_key: &<Polkadot as ChainCrypto>::AggKey,
		signature: &<Polkadot as ChainCrypto>::ThresholdSignature,
	) -> bool {
		// First check if the payload is still valid. If it is, check if the signature is still
		// valid
		(&call.threshold_signature_payload() == payload) &&
			<Polkadot as ChainCrypto>::verify_threshold_signature(
				current_key,
				payload,
				signature,
			)
	}
}

pub struct BtcTransactionBuilder;
impl TransactionBuilder<Bitcoin, BitcoinApi<BtcEnvironment>> for BtcTransactionBuilder {
	fn build_transaction(
		signed_call: &BitcoinApi<BtcEnvironment>,
	) -> <Bitcoin as ChainAbi>::Transaction {
		BitcoinTransactionData { encoded_transaction: signed_call.chain_encoded() }
	}

	fn refresh_unsigned_data(_unsigned_tx: &mut <Bitcoin as ChainAbi>::Transaction) {
		// Since BTC txs are chained and the subsequent tx depends on the success of the previous
		// one, changing the BTC tx fee will mean all subsequent txs are also invalid and so
		// refreshing btc tx is not trivial. We leave it a no-op for now.
	}

	fn is_valid_for_rebroadcast(
		_call: &BitcoinApi<BtcEnvironment>,
		_payload: &<Bitcoin as ChainCrypto>::Payload,
		_current_key: &<Bitcoin as ChainCrypto>::AggKey,
		_signature: &<Bitcoin as ChainCrypto>::ThresholdSignature,
	) -> bool {
		// The payload for a Bitcoin transaction will never change and so it doesnt need to be
		// checked here. We also dont need to check for the signature here because even if we are in
		// the next epoch and the key has changed, the old signature for the btc tx is still valid
		// since its based on those old input UTXOs. In fact, we never have to resign btc txs and
		// the btc tx is always valid as long as the input UTXOs are valid. Therefore, we don't have
		// to check anything here and just rebroadcast.
		true
	}
}

pub struct BlockAuthorRewardDistribution;

impl RewardsDistribution for BlockAuthorRewardDistribution {
	type Balance = FlipBalance;
	type Issuance = pallet_cf_flip::FlipIssuance<Runtime>;

	fn distribute() {
		let reward_amount = Emissions::current_authority_emission_per_block();
		if reward_amount != 0 {
			if let Some(current_block_author) = Authorship::author() {
				Flip::settle(&current_block_author, Self::Issuance::mint(reward_amount).into());
			} else {
				log::warn!("No block author for block {}.", System::current_block_number());
			}
		}
	}
}
pub struct RuntimeUpgradeManager;

impl RuntimeUpgrade for RuntimeUpgradeManager {
	fn do_upgrade(code: Vec<u8>) -> Result<PostDispatchInfo, DispatchErrorWithPostInfo> {
		System::set_code(frame_system::RawOrigin::Root.into(), code)
	}
}
pub struct EthEnvironment;

impl ReplayProtectionProvider<Ethereum> for EthEnvironment {
	fn replay_protection() -> EthereumReplayProtection {
		unimplemented!()
	}
}

impl EthEnvironmentProvider for EthEnvironment {
	fn token_address(asset: assets::eth::Asset) -> Option<eth::Address> {
		match asset {
			assets::eth::Asset::Eth => Some(ETHEREUM_ETH_ADDRESS),
			erc20 => Environment::supported_eth_assets(erc20).map(Into::into),
		}
	}

	fn contract_address(contract: EthereumContract) -> eth::Address {
		match contract {
			EthereumContract::StateChainGateway => Environment::state_chain_gateway_address(),
			EthereumContract::KeyManager => Environment::key_manager_address(),
			EthereumContract::Vault => Environment::eth_vault_address(),
		}
	}

	fn chain_id() -> eth::api::EthereumChainId {
		Environment::ethereum_chain_id()
	}

	fn next_nonce() -> u64 {
		Environment::next_ethereum_signature_nonce()
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct DotEnvironment;

impl ReplayProtectionProvider<Polkadot> for DotEnvironment {
	// Get the Environment values for vault_account, NetworkChoice and the next nonce for the
	// proxy_account
	fn replay_protection() -> PolkadotReplayProtection {
		PolkadotReplayProtection {
			genesis_hash: Environment::polkadot_genesis_hash(),
			nonce: Environment::next_polkadot_proxy_account_nonce(),
		}
	}
}

impl Get<RuntimeVersion> for DotEnvironment {
	fn get() -> RuntimeVersion {
		PolkadotChainTracking::chain_state().unwrap().tracked_data.runtime_version
	}
}

impl ChainEnvironment<cf_chains::dot::api::SystemAccounts, PolkadotAccountId> for DotEnvironment {
	fn lookup(query: cf_chains::dot::api::SystemAccounts) -> Option<PolkadotAccountId> {
		use crate::PolkadotVault;
		match query {
			cf_chains::dot::api::SystemAccounts::Proxy =>
				<PolkadotVault as KeyProvider<Polkadot>>::active_epoch_key()
					.map(|epoch_key| epoch_key.key),
			cf_chains::dot::api::SystemAccounts::Vault => Environment::polkadot_vault_account(),
		}
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct BtcEnvironment;

impl ReplayProtectionProvider<Bitcoin> for BtcEnvironment {
	// TODO: Implement replay protection for Bitcoin.
	fn replay_protection() {}
}

impl ChainEnvironment<UtxoSelectionType, SelectedUtxosAndChangeAmount> for BtcEnvironment {
	fn lookup(utxo_selection_type: UtxoSelectionType) -> Option<SelectedUtxosAndChangeAmount> {
		Environment::select_and_take_bitcoin_utxos(utxo_selection_type)
	}
}

impl ChainEnvironment<(), cf_chains::btc::AggKey> for BtcEnvironment {
	fn lookup(_: ()) -> Option<cf_chains::btc::AggKey> {
		<BitcoinVault as KeyProvider<Bitcoin>>::active_epoch_key().map(|epoch_key| epoch_key.key)
	}
}

pub struct EthVaultTransitionHandler;
impl VaultTransitionHandler<Ethereum> for EthVaultTransitionHandler {}

pub struct DotVaultTransitionHandler;
impl VaultTransitionHandler<Polkadot> for DotVaultTransitionHandler {
	fn on_new_vault() {
		Environment::reset_polkadot_proxy_account_nonce();
	}
}
pub struct BtcVaultTransitionHandler;
impl VaultTransitionHandler<Bitcoin> for BtcVaultTransitionHandler {}

pub struct TokenholderGovernanceBroadcaster;

impl TokenholderGovernanceBroadcaster {
	fn broadcast_gov_key<C, B>(maybe_old_key: Option<Vec<u8>>, new_key: Vec<u8>) -> Result<(), ()>
	where
		C: ChainAbi,
		B: Broadcaster<C>,
		<B as Broadcaster<C>>::ApiCall: cf_chains::SetGovKeyWithAggKey<C>,
	{
		let maybe_old_key = if let Some(old_key) = maybe_old_key {
			Some(Decode::decode(&mut &old_key[..]).or(Err(()))?)
		} else {
			None
		};
		let api_call = SetGovKeyWithAggKey::<C>::new_unsigned(
			maybe_old_key,
			Decode::decode(&mut &new_key[..]).or(Err(()))?,
		)?;
		B::threshold_sign_and_broadcast(api_call);
		Ok(())
	}

	fn is_govkey_compatible<C: ChainCrypto>(key: &[u8]) -> bool {
		C::GovKey::decode(&mut &key[..]).is_ok()
	}
}

impl BroadcastAnyChainGovKey for TokenholderGovernanceBroadcaster {
	fn broadcast_gov_key(
		chain: ForeignChain,
		maybe_old_key: Option<Vec<u8>>,
		new_key: Vec<u8>,
	) -> Result<(), ()> {
		match chain {
			ForeignChain::Ethereum =>
				Self::broadcast_gov_key::<Ethereum, EthereumBroadcaster>(maybe_old_key, new_key),
			ForeignChain::Polkadot =>
				Self::broadcast_gov_key::<Polkadot, PolkadotBroadcaster>(maybe_old_key, new_key),
			ForeignChain::Bitcoin => Err(()),
		}
	}

	fn is_govkey_compatible(chain: ForeignChain, key: &[u8]) -> bool {
		match chain {
			ForeignChain::Ethereum => Self::is_govkey_compatible::<Ethereum>(key),
			ForeignChain::Polkadot => Self::is_govkey_compatible::<Polkadot>(key),
			ForeignChain::Bitcoin => false,
		}
	}
}

impl CommKeyBroadcaster for TokenholderGovernanceBroadcaster {
	fn broadcast(new_key: <Ethereum as ChainCrypto>::GovKey) {
		EthereumBroadcaster::threshold_sign_and_broadcast(
			SetCommKeyWithAggKey::<Ethereum>::new_unsigned(new_key),
			None::<RuntimeCall>,
		);
	}
}

#[macro_export]
macro_rules! impl_deposit_api_for_anychain {
	( $t: ident, $(($chain: ident, $pallet: ident)),+ ) => {
		impl DepositApi<AnyChain> for $t {
			type AccountId = <Runtime as frame_system::Config>::AccountId;

			fn request_liquidity_deposit_address(
				lp_account: Self::AccountId,
				source_asset: Asset,
			) -> Result<(ChannelId, ForeignChainAddress), DispatchError> {
				match source_asset.into() {
					$(
						ForeignChain::$chain =>
							$pallet::request_liquidity_deposit_address(
								lp_account,
								source_asset.try_into().unwrap(),
							),
					)+
				}
			}

			fn request_swap_deposit_address(
				source_asset: Asset,
				destination_asset: Asset,
				destination_address: ForeignChainAddress,
				broker_commission_bps: BasisPoints,
				broker_id: Self::AccountId,
				channel_metadata: Option<CcmChannelMetadata>,
			) -> Result<(ChannelId, ForeignChainAddress), DispatchError> {
				match source_asset.into() {
					$(
						ForeignChain::$chain => $pallet::request_swap_deposit_address(
							source_asset.try_into().unwrap(),
							destination_asset,
							destination_address,
							broker_commission_bps,
							broker_id,
							channel_metadata,
						),
					)+
				}
			}

			fn expire_channel(address: ForeignChainAddress) {
				if address.chain() == ForeignChain::Bitcoin {
					Environment::cleanup_bitcoin_deposit_address_details(address.clone().try_into().expect("Checked for address compatibility"));
				}
				match address.chain() {
					$(
						ForeignChain::$chain => {
							<$pallet as DepositApi<$chain>>::expire_channel(
								address.try_into().expect("Checked for address compatibility")
							);
						},
					)+
				}
			}
		}
	}
}

#[macro_export]
macro_rules! impl_egress_api_for_anychain {
	( $t: ident, $(($chain: ident, $pallet: ident)),+ ) => {
		impl EgressApi<AnyChain> for $t {
			fn schedule_egress(
				asset: Asset,
				amount: <AnyChain as Chain>::ChainAmount,
				destination_address: <AnyChain as Chain>::ChainAccount,
				maybe_message: Option<CcmDepositMetadata>,
			) -> EgressId {
				match asset.into() {
					$(
						ForeignChain::$chain => $pallet::schedule_egress(
							asset.try_into().expect("Checked for asset compatibility"),
							amount.try_into().expect("Checked for amount compatibility"),
							destination_address
								.try_into()
								.expect("This address cast is ensured to succeed."),
							maybe_message,
						),

					)+
				}
			}
		}
	}
}

pub struct AnyChainIngressEgressHandler;
impl_deposit_api_for_anychain!(
	AnyChainIngressEgressHandler,
	(Ethereum, EthereumIngressEgress),
	(Polkadot, PolkadotIngressEgress),
	(Bitcoin, BitcoinIngressEgress)
);

impl_egress_api_for_anychain!(
	AnyChainIngressEgressHandler,
	(Ethereum, EthereumIngressEgress),
	(Polkadot, PolkadotIngressEgress),
	(Bitcoin, BitcoinIngressEgress)
);

pub struct EthDepositHandler;
impl DepositHandler<Ethereum> for EthDepositHandler {}

pub struct DotDepositHandler;
impl DepositHandler<Polkadot> for DotDepositHandler {}

pub struct BtcDepositHandler;
impl DepositHandler<Bitcoin> for BtcDepositHandler {
	fn on_deposit_made(
		utxo_id: <Bitcoin as Chain>::DepositDetails,
		amount: <Bitcoin as Chain>::ChainAmount,
		address: <Bitcoin as Chain>::ChainAccount,
		_asset: <Bitcoin as Chain>::ChainAsset,
	) {
		Environment::add_bitcoin_utxo_to_list(amount, utxo_id, address)
	}

	fn on_channel_opened(
		script_pubkey: ScriptPubkey,
		salt: ChannelId,
	) -> Result<(), DispatchError> {
		Environment::add_details_for_btc_deposit_script(
			script_pubkey,
			salt.try_into().expect("The salt/channel_id is not expected to exceed u32 max"), /* Todo: Confirm
			                                                                                  * this assumption.
			                                                                                  * Consider this in
			                                                                                  * conjunction with
			                                                                                  * #2354 */
			BitcoinVault::vaults(Validator::epoch_index())
				.ok_or(DispatchError::Other("No vault for epoch"))?
				.public_key
				.current,
		);
		Ok(())
	}
}

pub struct ChainAddressConverter;

impl AddressConverter for ChainAddressConverter {
	fn to_encoded_address(address: ForeignChainAddress) -> EncodedAddress {
		to_encoded_address(address, Environment::network_environment)
	}

	fn try_from_encoded_address(
		encoded_address: EncodedAddress,
	) -> Result<ForeignChainAddress, ()> {
		try_from_encoded_address(encoded_address, Environment::network_environment)
	}
}

pub struct BroadcastReadyProvider;
impl OnBroadcastReady<Ethereum> for BroadcastReadyProvider {
	type ApiCall = EthereumApi<EthEnvironment>;
}
impl OnBroadcastReady<Polkadot> for BroadcastReadyProvider {
	type ApiCall = PolkadotApi<DotEnvironment>;
}
impl OnBroadcastReady<Bitcoin> for BroadcastReadyProvider {
	type ApiCall = BitcoinApi<BtcEnvironment>;

	fn on_broadcast_ready(api_call: &Self::ApiCall) {
		match api_call {
			BitcoinApi::BatchTransfer(batch_transfer) => {
				let tx_id = batch_transfer.bitcoin_transaction.txid();
				let outputs = batch_transfer.bitcoin_transaction.outputs.clone();
				let output_len = outputs.len();
				let vout = output_len - 1;
				let change_output = outputs.get(vout).unwrap();
				Environment::add_bitcoin_change_utxo(
					change_output.amount,
					UtxoId { tx_id, vout: vout as u32 },
					batch_transfer.change_utxo_key,
				);
			},
			_ => unreachable!(),
		}
	}
}

pub struct BitcoinFeeGetter;
impl cf_traits::GetBitcoinFeeInfo for BitcoinFeeGetter {
	fn bitcoin_fee_info() -> BitcoinFeeInfo {
		BitcoinChainTracking::chain_state().unwrap().tracked_data.btc_fee_info
	}
}

pub struct ValidatorRoleQualification;

impl QualifyNode<<Runtime as Chainflip>::ValidatorId> for ValidatorRoleQualification {
	fn is_qualified(id: &<Runtime as Chainflip>::ValidatorId) -> bool {
		AccountRoles::has_account_role(id, AccountRole::Validator)
	}
}
