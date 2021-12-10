//! Configuration, utilities and helpers for the Chainflip runtime.
use super::{
	AccountId, Call, Emissions, Environment, Flip, FlipBalance, Online, Reputation, Rewards,
	Runtime, Validator, Vaults, Witnesser,
};
use crate::{BlockNumber, EmergencyRotationPercentageRange, HeartbeatBlockInterval};
use cf_chains::{
	eth::{
		self, register_claim::RegisterClaim, set_agg_key_with_agg_key::SetAggKeyWithAggKey,
		update_flip_supply::UpdateFlipSupply, Address, ChainflipContractCall,
	},
	Chain, ChainCrypto, Ethereum,
};
use cf_traits::{
	offline_conditions::{OfflineCondition, ReputationPoints},
	BlockEmissions, BondRotation, Chainflip, ChainflipAccount, ChainflipAccountState,
	ChainflipAccountStore, EmergencyRotation, EmissionsTrigger, EpochInfo, EpochTransitionHandler,
	Heartbeat, IsOnline, Issuance, KeyProvider, NetworkState, RewardRollover, SigningContext,
	StakeHandler, StakeTransfer, VaultRotationHandler,
};
use codec::{Decode, Encode};
use frame_support::{instances::*, weights::Weight};
use pallet_cf_auction::{HandleStakes, VaultRotationEventHandler};
use pallet_cf_broadcast::BroadcastConfig;
use pallet_cf_validator::PercentageRange;
use sp_runtime::{
	traits::{AtLeast32BitUnsigned, UniqueSaturatedFrom},
	RuntimeDebug,
};
use sp_std::{cmp::min, convert::TryInto, marker::PhantomData, prelude::*};

use sp_io::hashing::twox_128;

impl Chainflip for Runtime {
	type Call = Call;
	type Amount = FlipBalance;
	type ValidatorId = <Self as frame_system::Config>::AccountId;
	type KeyId = Vec<u8>;
	type EnsureWitnessed = pallet_cf_witnesser::EnsureWitnessed;
	type EpochInfo = Validator;
}

pub struct ChainflipEpochTransitions;

/// Trigger emissions on epoch transitions.
impl EpochTransitionHandler for ChainflipEpochTransitions {
	type ValidatorId = AccountId;
	type Amount = FlipBalance;

	fn on_new_epoch(
		old_validators: &[Self::ValidatorId],
		new_validators: &[Self::ValidatorId],
		new_bond: Self::Amount,
	) {
		// Calculate block emissions on every epoch
		<Emissions as BlockEmissions>::calculate_block_emissions();
		// Process any outstanding emissions.
		<Emissions as EmissionsTrigger>::trigger_emissions();
		// Rollover the rewards.
		<Rewards as RewardRollover>::rollover(new_validators).unwrap_or_else(|err| {
			log::error!("Unable to process rewards rollover: {:?}!", err);
		});
		// Update the the bond of all validators for the new epoch
		<Flip as BondRotation>::update_validator_bonds(new_validators, new_bond);
		// Update the list of validators in the witnesser.
		<Witnesser as EpochTransitionHandler>::on_new_epoch(
			old_validators,
			new_validators,
			new_bond,
		);

		<AccountStateManager<Runtime> as EpochTransitionHandler>::on_new_epoch(
			old_validators,
			new_validators,
			new_bond,
		);
	}
}

pub struct AccountStateManager<T>(PhantomData<T>);

impl<T: Chainflip> EpochTransitionHandler for AccountStateManager<T> {
	type ValidatorId = AccountId;
	type Amount = T::Amount;

	fn on_new_epoch(
		_old_validators: &[Self::ValidatorId],
		new_validators: &[Self::ValidatorId],
		_new_bid: Self::Amount,
	) {
		// Update the last active epoch for the new validating set
		let epoch_index = Validator::epoch_index();
		for validator in new_validators {
			ChainflipAccountStore::<Runtime>::update_last_active_epoch(&validator, epoch_index);
		}
	}
}

pub struct ChainflipStakeHandler;
impl StakeHandler for ChainflipStakeHandler {
	type ValidatorId = AccountId;
	type Amount = FlipBalance;

	fn stake_updated(validator_id: &Self::ValidatorId, new_total: Self::Amount) {
		HandleStakes::<Runtime>::stake_updated(validator_id, new_total);
	}
}

pub struct ChainflipVaultRotationHandler;
impl VaultRotationHandler for ChainflipVaultRotationHandler {
	type ValidatorId = AccountId;

	fn vault_rotation_aborted() {
		VaultRotationEventHandler::<Runtime>::vault_rotation_aborted();
	}
}

trait RewardDistribution {
	type EpochInfo: EpochInfo;
	type StakeTransfer: StakeTransfer;
	type ValidatorId;
	type BlockNumber;
	type FlipBalance: UniqueSaturatedFrom<Self::BlockNumber> + AtLeast32BitUnsigned;
	/// An implementation of the [Issuance] trait.
	type Issuance: Issuance;

	/// Distribute rewards
	fn distribute_rewards(backup_validators: &[&Self::ValidatorId]) -> Weight;
}

struct BackupValidatorEmissions;

impl RewardDistribution for BackupValidatorEmissions {
	type EpochInfo = Validator;
	type StakeTransfer = Flip;
	type ValidatorId = AccountId;
	type BlockNumber = BlockNumber;
	type FlipBalance = FlipBalance;
	type Issuance = pallet_cf_flip::FlipIssuance<Runtime>;

	// This is called on each heartbeat interval
	fn distribute_rewards(backup_validators: &[&Self::ValidatorId]) -> Weight {
		// The current minimum active bid
		let minimum_active_bid = Self::EpochInfo::bond();
		// Our emission cap for this heartbeat interval
		let emissions_cap = Emissions::backup_validator_emission_per_block() *
			Self::FlipBalance::unique_saturated_from(HeartbeatBlockInterval::get());

		// Emissions for this heartbeat interval for the active set
		let validator_rewards = Emissions::validator_emission_per_block() *
			Self::FlipBalance::unique_saturated_from(HeartbeatBlockInterval::get());

		// The average validator emission
		let average_validator_reward: Self::FlipBalance = validator_rewards /
			Self::FlipBalance::unique_saturated_from(Self::EpochInfo::current_validators().len());

		let mut total_rewards = 0;

		// Calculate rewards for each backup validator and total rewards for capping
		let mut rewards: Vec<(&Self::ValidatorId, Self::FlipBalance)> = backup_validators
			.iter()
			.map(|backup_validator| {
				let backup_validator_stake =
					Self::StakeTransfer::stakeable_balance(*backup_validator);
				let reward_scaling_factor =
					min(1, (backup_validator_stake / minimum_active_bid) ^ 2);
				let reward = (reward_scaling_factor * average_validator_reward * 8) / 10;
				total_rewards += reward;
				(*backup_validator, reward)
			})
			.collect();

		// Cap if needed
		if total_rewards > emissions_cap {
			rewards = rewards
				.into_iter()
				.map(|(validator_id, reward)| {
					(validator_id, (reward * emissions_cap) / total_rewards)
				})
				.collect();
		}

		// Distribute rewards one by one
		// N.B. This could be more optimal
		for (validator_id, reward) in rewards {
			Flip::settle(&validator_id, Self::Issuance::mint(reward).into());
		}

		0
	}
}

pub struct ChainflipHeartbeat;

impl Heartbeat for ChainflipHeartbeat {
	type ValidatorId = AccountId;
	type BlockNumber = BlockNumber;

	fn heartbeat_submitted(
		validator_id: &Self::ValidatorId,
		block_number: Self::BlockNumber,
	) -> Weight {
		<Reputation as Heartbeat>::heartbeat_submitted(validator_id, block_number)
	}

	fn on_heartbeat_interval(network_state: NetworkState<Self::ValidatorId>) -> Weight {
		// Reputation depends on heartbeats
		let mut weight = <Reputation as Heartbeat>::on_heartbeat_interval(network_state.clone());

		// We pay rewards to online backup validators on each heartbeat interval
		let backup_validators: Vec<&Self::ValidatorId> = network_state
			.online
			.iter()
			.filter(|account_id| {
				ChainflipAccountStore::<Runtime>::get(*account_id).state ==
					ChainflipAccountState::Backup
			})
			.collect();

		BackupValidatorEmissions::distribute_rewards(&backup_validators);

		// Check the state of the network and if we are within the emergency rotation range
		// then issue an emergency rotation request
		let PercentageRange { top, bottom } = EmergencyRotationPercentageRange::get();
		let percent_online = network_state.percentage_online() as u8;
		if percent_online >= bottom && percent_online <= top {
			weight += <Validator as EmergencyRotation>::request_emergency_rotation();
		}

		weight
	}
}

/// Returns a scaled index based on an input seed
pub fn get_random_id_by_seed_in_range(seed: Vec<u8>, max: usize) -> usize {
	let hash = twox_128(&seed);
	let seed = u32::from_be_bytes([hash[0], hash[1], hash[2], hash[3]]);
	let id = seed % max as u32;
	id as usize
}

/// Select the next signer
pub fn select_signer<SignerId: Clone, T: IsOnline<ValidatorId = SignerId>>(
	validators: Vec<(SignerId, ())>,
	seed: Vec<u8>,
) -> Option<SignerId> {
	// Get all online validators
	let online_validators =
		validators.iter().filter(|(id, _)| T::is_online(id)).collect::<Vec<_>>();
	let number_of_online_validators = online_validators.len();
	// Check if there is someone online
	if number_of_online_validators == 0 {
		return None
	}
	// Get a a pseudo random id by which we choose the next validator
	let the_chosen_one = get_random_id_by_seed_in_range(seed, number_of_online_validators);
	if let Some(signer) = online_validators.get(the_chosen_one) {
		Some(signer.0.clone())
	} else {
		None
	}
}
/// A very basic but working implementation of signer nomination.
///
/// For a single signer, takes the first online validator in the validator lookup map.
///
/// For multiple signers, takes the first N online validators where N is signing consensus
/// threshold.
pub struct BasicSignerNomination;

impl cf_traits::SignerNomination for BasicSignerNomination {
	type SignerId = AccountId;

	fn nomination_with_seed(seed: Vec<u8>) -> Option<Self::SignerId> {
		let validators =
			pallet_cf_validator::ValidatorLookup::<Runtime>::iter().collect::<Vec<_>>();
		select_signer::<Self::SignerId, Online>(validators, seed)
	}

	fn threshold_nomination_with_seed(_seed: u64) -> Vec<Self::SignerId> {
		let threshold = pallet_cf_witnesser::ConsensusThreshold::<Runtime>::get();
		pallet_cf_validator::ValidatorLookup::<Runtime>::iter()
			.filter_map(|(id, _)| {
				if <Online as cf_traits::IsOnline>::is_online(&id) {
					Some(id)
				} else {
					None
				}
			})
			.take(threshold as usize)
			.collect()
	}
}

// Supported Ethereum signing operations.
#[derive(Encode, Decode, Clone, RuntimeDebug, PartialEq, Eq)]
pub enum EthereumSigningContext {
	PostClaimSignature(RegisterClaim),
	SetAggKeyWithAggKeyBroadcast(SetAggKeyWithAggKey),
	UpdateFlipSupply(UpdateFlipSupply),
}

impl From<RegisterClaim> for EthereumSigningContext {
	fn from(call: RegisterClaim) -> Self {
		EthereumSigningContext::PostClaimSignature(call)
	}
}

impl From<SetAggKeyWithAggKey> for EthereumSigningContext {
	fn from(call: SetAggKeyWithAggKey) -> Self {
		EthereumSigningContext::SetAggKeyWithAggKeyBroadcast(call)
	}
}

impl From<UpdateFlipSupply> for EthereumSigningContext {
	fn from(call: UpdateFlipSupply) -> Self {
		EthereumSigningContext::UpdateFlipSupply(call)
	}
}

impl SigningContext<Runtime> for EthereumSigningContext {
	type Chain = cf_chains::Ethereum;
	type Callback = Call;
	type ThresholdSignatureOrigin = pallet_cf_threshold_signature::Origin<Runtime, Instance1>;

	fn get_payload(&self) -> <Self::Chain as ChainCrypto>::Payload {
		match self {
			Self::PostClaimSignature(ref claim) => claim.signing_payload(),
			Self::SetAggKeyWithAggKeyBroadcast(ref call) => call.signing_payload(),
			Self::UpdateFlipSupply(ref call) => call.signing_payload(),
		}
	}

	fn resolve_callback(
		&self,
		signature: <Self::Chain as ChainCrypto>::ThresholdSignature,
	) -> Self::Callback {
		match self {
			Self::PostClaimSignature(claim) =>
				pallet_cf_staking::Call::<Runtime>::post_claim_signature(
					claim.node_id.into(),
					signature,
				)
				.into(),
			Self::SetAggKeyWithAggKeyBroadcast(call) => Call::EthereumBroadcaster(
				pallet_cf_broadcast::Call::<_, _>::start_broadcast(contract_call_to_unsigned_tx(
					call.clone(),
					&signature,
					Environment::key_manager_address().into(),
				)),
			),
			Self::UpdateFlipSupply(call) =>
				Call::EthereumBroadcaster(pallet_cf_broadcast::Call::<_, _>::start_broadcast(
					contract_call_to_unsigned_tx(
						call.clone(),
						&signature,
						Environment::stake_manager_address().into(),
					),
				)),
		}
	}
}

fn contract_call_to_unsigned_tx<C: ChainflipContractCall>(
	call: C,
	signature: &eth::SchnorrVerificationComponents,
	contract_address: Address,
) -> eth::UnsignedTransaction {
	eth::UnsignedTransaction {
		chain_id: Environment::ethereum_chain_id(),
		contract: contract_address,
		data: call.abi_encode_with_signature(signature),
		..Default::default()
	}
}

pub struct EthereumBroadcastConfig;

impl BroadcastConfig for EthereumBroadcastConfig {
	type Chain = Ethereum;
	type UnsignedTransaction = eth::UnsignedTransaction;
	type SignedTransaction = eth::RawSignedTransaction;
	type TransactionHash = [u8; 32];
	type SignerId = eth::Address;

	fn verify_transaction(
		unsigned_tx: &Self::UnsignedTransaction,
		signed_tx: &Self::SignedTransaction,
		address: &Self::SignerId,
	) -> Option<()> {
		eth::verify_transaction(unsigned_tx, signed_tx, address)
			.map_err(|e| log::info!("Ethereum signed transaction verification failed: {:?}.", e))
			.ok()
	}
}

/// Simple Ethereum-specific key provider that reads from the vault.
pub struct EthereumKeyProvider;

impl KeyProvider<Ethereum> for EthereumKeyProvider {
	type KeyId = Vec<u8>;

	fn current_key_id() -> Self::KeyId {
		Vaults::vaults(Validator::epoch_index(), <Ethereum as Chain>::CHAIN_ID)
			.expect("Ethereum is always supported.")
			.public_key
	}

	fn current_key() -> <Ethereum as ChainCrypto>::AggKey {
		Vaults::vaults(Validator::epoch_index(), <Ethereum as Chain>::CHAIN_ID)
			.expect("Ethereum is always supported.")
			.public_key
			.try_into()
			.expect("TODO: make it so this call can't fail.")
	}
}

/// Checks if the caller can execute free transactions
pub struct WaivedFees;

impl cf_traits::WaivedFees for WaivedFees {
	type AccountId = AccountId;
	type Call = Call;

	fn should_waive_fees(call: &Self::Call, caller: &Self::AccountId) -> bool {
		let is_gov_call = match call {
			Call::Governance(_) => true,
			_ => false,
		};
		if is_gov_call {
			return super::Governance::members().contains(caller)
		}
		return false
	}
}

pub struct OfflinePenalty;

impl cf_traits::offline_conditions::OfflinePenalty for OfflinePenalty {
	fn penalty(condition: &OfflineCondition) -> ReputationPoints {
		match condition {
			OfflineCondition::BroadcastOutputFailed => 15,
			OfflineCondition::ParticipateSigningFailed => 15,
			OfflineCondition::NotEnoughPerformanceCredits => 100,
		}
	}
}
