//! Configuration, utilities and helpers for the Chainflip runtime.
use super::{
	AccountId, Call, Emissions, FlipBalance, Reputation, Rewards, Runtime, Validator, Vaults,
	Witnesser,
};
use cf_chains::{Ethereum, eth::{self, ChainflipContractCall, register_claim::RegisterClaim}};
use cf_traits::{Chainflip, EmissionsTrigger, KeyProvider, SigningContext};
use codec::{Decode, Encode};
use frame_support::debug;
use pallet_cf_broadcast::BroadcastConfig;
use pallet_cf_validator::EpochTransitionHandler;
use sp_core::H256;
use sp_runtime::{DispatchError, RuntimeDebug};
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
		// Update the list of validators in reputation
		<Reputation as EpochTransitionHandler>::on_new_epoch(new_validators, new_bond);
		// Update the list of validators in the witnesser.
		<Witnesser as EpochTransitionHandler>::on_new_epoch(new_validators, new_bond)
	}
}

pub struct BasicSignerNomination;

impl cf_traits::SignerNomination for BasicSignerNomination {
	type SignerId = AccountId;

	fn nomination_with_seed(seed: u64) -> Self::SignerId {
		todo!()
	}

	fn threshold_nomination_with_seed(seed: u64) -> Vec<Self::SignerId> {
		todo!()
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
	type Signature = eth::SchnorrSignature;
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
				pallet_cf_staking::Call::<Runtime>::post_claim_signature(claim.node_id.into(), signature).into()
			}
			Self::Broadcast(contract_call) => {
				let mut contract_call = contract_call.clone();
				contract_call.sign(&signature);
				let unsigned_tx = eth::UnsignedTransaction {
					chain_id: eth::CHAIN_ID_RINKEBY,
					contract: eth::stake_manager_contract_address().into(),
					data: contract_call.abi_encoded(),
					..Default::default()
				};
				pallet_cf_broadcast::Call::<Runtime, pallet_cf_broadcast::Instance0>::start_broadcast(unsigned_tx).into()
			}
		}
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
		unsigned_tx: &Self::UnsignedTransaction,
		signed_tx: &Self::SignedTransaction,
	) -> Option<()> {
		todo!()
	}
}

pub struct VaultKeyProvider<T>(PhantomData<T>);

impl<T: pallet_cf_vaults::Config> KeyProvider<Ethereum> for VaultKeyProvider<T> {
	type KeyId = T::PublicKey;

	fn current_key() -> Self::KeyId {
		pallet_cf_vaults::Pallet::<T>::eth_vault().new_key
	}
}
