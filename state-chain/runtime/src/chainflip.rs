//! Configuration, utilities and helpers for the Chainflip runtime.
use super::{
	AccountId, Call, Emissions, Environment, Flip, FlipBalance, Online, Reputation, Rewards,
	Runtime, Validator, Vaults, Witnesser,
};
use crate::{BlockNumber, EmergencyRotationPercentageTrigger, HeartbeatBlockInterval};
use cf_chains::{
	eth::{
		self, register_claim::RegisterClaim, set_agg_key_with_agg_key::SetAggKeyWithAggKey,
		ChainflipContractCall,
	},
	Ethereum,
};
use cf_traits::{
	BlockEmissions, BondRotation, Chainflip, ChainflipAccount, ChainflipAccountState,
	ChainflipAccountStore, EmergencyRotation, EmissionsTrigger, EpochInfo, EpochTransitionHandler,
	Heartbeat, Issuance, KeyProvider, NetworkState, RewardRollover, SigningContext, StakeHandler,
	StakeTransfer, VaultRotationHandler,
};
use codec::{Decode, Encode};
use frame_support::{debug, weights::Weight};
use pallet_cf_auction::{HandleStakes, VaultRotationEventHandler};
use pallet_cf_broadcast::BroadcastConfig;
use sp_core::{H160, H256};
use sp_runtime::traits::{AtLeast32BitUnsigned, UniqueSaturatedFrom};
use sp_runtime::RuntimeDebug;
use sp_std::cmp::min;
use sp_std::prelude::*;

impl Chainflip for Runtime {
	type Call = Call;
	type Amount = FlipBalance;
	type ValidatorId = <Self as frame_system::Config>::AccountId;
	type KeyId = Vec<u8>;
	type EnsureWitnessed = pallet_cf_witnesser::EnsureWitnessed;
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
			debug::error!("Unable to process rewards rollover: {:?}!", err);
		});
		// Update the the bond of all validators for the new epoch
		<Flip as BondRotation>::update_validator_bonds(new_validators, new_bond);
		// Update the list of validators in reputation
		<Online as EpochTransitionHandler>::on_new_epoch(old_validators, new_validators, new_bond);
		// Update the list of validators in the witnesser.
		<Witnesser as EpochTransitionHandler>::on_new_epoch(
			old_validators,
			new_validators,
			new_bond,
		)
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

	fn penalise(bad_validators: &[Self::ValidatorId]) {
		VaultRotationEventHandler::<Runtime>::penalise(bad_validators);
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
		let emissions_cap = Emissions::backup_validator_emission_per_block()
			* Self::FlipBalance::unique_saturated_from(HeartbeatBlockInterval::get());

		// Emissions for this heartbeat interval for the active set
		let validator_rewards = Emissions::validator_emission_per_block()
			* Self::FlipBalance::unique_saturated_from(HeartbeatBlockInterval::get());

		// The average validator emission
		let average_validator_reward: Self::FlipBalance = validator_rewards
			/ Self::FlipBalance::unique_saturated_from(Self::EpochInfo::current_validators().len());

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

	fn heartbeat_submitted(validator_id: &Self::ValidatorId) -> Weight {
		<Reputation as Heartbeat>::heartbeat_submitted(validator_id)
	}

	fn on_heartbeat_interval(network_state: NetworkState<Self::ValidatorId>) -> Weight {
		// Reputation depends on heartbeats
		let mut weight = <Reputation as Heartbeat>::on_heartbeat_interval(network_state.clone());

		// We pay rewards to online backup validators on each heartbeat interval
		let backup_validators: Vec<&Self::ValidatorId> = network_state
			.online
			.iter()
			.filter(|account_id| {
				ChainflipAccountStore::<Runtime>::get(*account_id).state
					== ChainflipAccountState::Backup
			})
			.collect();

		BackupValidatorEmissions::distribute_rewards(&backup_validators);

		// Check the state of the network and if we are below the emergency rotation trigger
		// then issue an emergency rotation request
		if network_state.percentage_online() < EmergencyRotationPercentageTrigger::get() as u32 {
			weight += <Validator as EmergencyRotation>::request_emergency_rotation();
		}

		weight
	}
}

/// A very basic but working implementation of signer nomination.
///
/// For a single signer, takes the first online validator in the validator lookup map.
///
/// For multiple signers, takes the first N online validators where N is signing consensus threshold.
pub struct BasicSignerNomination;

impl cf_traits::SignerNomination for BasicSignerNomination {
	type SignerId = AccountId;

	fn nomination_with_seed(_seed: u64) -> Self::SignerId {
		pallet_cf_validator::ValidatorLookup::<Runtime>::iter()
			.skip_while(|(id, _)| !<Online as cf_traits::IsOnline>::is_online(id))
			.take(1)
			.collect::<Vec<_>>()
			.first()
			.expect("Can only panic if all validators are offline.")
			.0
			.clone()
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

impl SigningContext<Runtime> for EthereumSigningContext {
	type Chain = cf_chains::Ethereum;
	type Payload = H256;
	type Signature = eth::SchnorrVerificationComponents;
	type Callback = Call;

	fn get_payload(&self) -> Self::Payload {
		match self {
			Self::PostClaimSignature(ref claim) => claim.signing_payload(),
			Self::SetAggKeyWithAggKeyBroadcast(ref call) => call.signing_payload(),
		}
	}

	fn resolve_callback(&self, signature: Self::Signature) -> Self::Callback {
		match self {
			Self::PostClaimSignature(claim) => {
				pallet_cf_staking::Call::<Runtime>::post_claim_signature(
					claim.node_id.into(),
					signature,
				)
				.into()
			}
			Self::SetAggKeyWithAggKeyBroadcast(call) => Call::EthereumBroadcaster(
				pallet_cf_broadcast::Call::<_, _>::start_broadcast(contract_call_to_unsigned_tx(
					call.clone(),
					&signature,
					Environment::key_manager_address(),
				)),
			),
		}
	}
}

fn contract_call_to_unsigned_tx<C: ChainflipContractCall>(
	call: C,
	signature: &eth::SchnorrVerificationComponents,
	contract_address: H160,
) -> eth::UnsignedTransaction {
	eth::UnsignedTransaction {
		chain_id: Environment::ethereum_chain_id(),
		contract: contract_address,
		data: call.abi_encode_with_signature(signature),
		..Default::default()
	}
}

pub struct EthereumBroadcastConfig;

impl BroadcastConfig<Runtime> for EthereumBroadcastConfig {
	type Chain = Ethereum;
	type UnsignedTransaction = eth::UnsignedTransaction;
	type SignedTransaction = eth::RawSignedTransaction;
	type TransactionHash = [u8; 32];

	fn verify_transaction(
		signer: &<Runtime as Chainflip>::ValidatorId,
		_unsigned_tx: &Self::UnsignedTransaction,
		signed_tx: &Self::SignedTransaction,
	) -> Option<()> {
		eth::verify_raw(signed_tx, signer)
			.map_err(|e| {
				frame_support::debug::info!(
					"Ethereum signed transaction verification failed: {:?}.",
					e
				)
			})
			.ok()
	}
}

/// Simple Ethereum-specific key provider that reads from the vault.
pub struct EthereumKeyProvider;

impl KeyProvider<Ethereum> for EthereumKeyProvider {
	type KeyId = Vec<u8>;

	fn current_key() -> Self::KeyId {
		Vaults::vaults(<Ethereum as cf_chains::Chain>::CHAIN_ID)
			.expect("Ethereum is always supported.")
			.public_key
	}
}
