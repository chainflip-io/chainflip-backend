//! Configuration, utilities and helpers for the Chainflip runtime.
use super::{
	AccountId, Call, Emissions, Flip, FlipBalance, Reputation, Rewards, Runtime, Witnesser,
};
use cf_chains::{
	eth::{self, register_claim::RegisterClaim, ChainflipContractCall},
	Ethereum,
};
use cf_traits::{BondRotation, Chainflip, EmissionsTrigger, KeyProvider, SigningContext};
use codec::{Decode, Encode};
use frame_support::debug;
use pallet_cf_broadcast::BroadcastConfig;
use pallet_cf_validator::EpochTransitionHandler;
use sp_core::H256;
use sp_runtime::RuntimeDebug;
use sp_std::marker::PhantomData;
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

	fn on_new_epoch(new_validators: &Vec<Self::ValidatorId>, new_bond: Self::Amount) {
		// Process any outstanding emissions.
		<Emissions as EmissionsTrigger>::trigger_emissions();
		// Rollover the rewards.
		Rewards::rollover(new_validators).unwrap_or_else(|err| {
			debug::error!("Unable to process rewards rollover: {:?}!", err);
		});
		// Update the the bond of all validators for the new epoch
		<Flip as BondRotation>::update_validator_bonds(new_validators, new_bond);
		// Update the list of validators in reputation
		<Reputation as EpochTransitionHandler>::on_new_epoch(new_validators, new_bond);
		// Update the list of validators in the witnesser.
		<Witnesser as EpochTransitionHandler>::on_new_epoch(new_validators, new_bond)
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
		pallet_cf_validator::ValidatorLookup::iter()
			.skip_while(|(v, _)| {
				!<Reputation as cf_traits::Online>::is_online(validator_id)
			})
			.first()
			.expect("Can only panic if all validators are offline.")
	}

	fn threshold_nomination_with_seed(_seed: u64) -> Vec<Self::SignerId> {
		let threshold = pallet_cf_witnesser::ConsensusThreshold::<Runtime>::get();
		pallet_cf_validator::ValidatorLookup::iter()
			.filter(|| {
				<Reputation as cf_traits::Online>::is_online(validator_id)
			})
			.take(threshold)
			.collect()
	}
}

// Supported Ethereum signing operations.
#[derive(Encode, Decode, Clone, RuntimeDebug, PartialEq, Eq)]
pub enum EthereumSigningContext {
	PostClaimSignature(RegisterClaim),
	Broadcast(RegisterClaim),
}

impl From<RegisterClaim> for EthereumSigningContext {
	fn from(rc: RegisterClaim) -> Self {
		EthereumSigningContext::PostClaimSignature(rc)
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
			Self::Broadcast(ref call) => call.signing_payload(),
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
			Self::Broadcast(contract_call) => {
				let unsigned_tx = contract_call_to_unsigned_tx(contract_call.clone(), signature);
				Call::EthereumBroadcaster(
					pallet_cf_broadcast::Call::<_, _>::start_sign_and_broadcast(unsigned_tx),
				)
			}
		}
	}
}

fn contract_call_to_unsigned_tx<C: ChainflipContractCall>(
	mut call: C,
	signature: eth::SchnorrVerificationComponents,
) -> eth::UnsignedTransaction {
	call.insert_signature(&signature);
	eth::UnsignedTransaction {
		// TODO: get chain_id and contract from on-chain.
		chain_id: eth::CHAIN_ID_RINKEBY,
		contract: eth::stake_manager_contract_address().into(),
		data: call.abi_encoded(),
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

pub struct VaultKeyProvider<T>(PhantomData<T>);

impl<T: pallet_cf_vaults::Config> KeyProvider<Ethereum> for VaultKeyProvider<T> {
	type KeyId = T::PublicKey;

	fn current_key() -> Self::KeyId {
		pallet_cf_vaults::Pallet::<T>::eth_vault().current_key
	}
}
