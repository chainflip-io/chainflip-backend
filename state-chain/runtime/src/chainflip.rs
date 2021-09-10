//! Configuration, utilities and helpers for the Chainflip runtime.
use super::{AccountId, Emissions, FlipBalance, Reputation, Rewards, Runtime, Call, Validator, Witnesser};
use cf_chains::eth;
use cf_traits::{EmissionsTrigger, KeyProvider};
use frame_support::debug;
use pallet_cf_validator::EpochTransitionHandler;
use sp_std::vec::Vec;

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

// Supported Ethereum transactions.
#[derive(Encode, Decode, Clone, RuntimeDebug, PartialEq, Eq)]
pub enum EthereumSigner {
	RegisterClaim(register_claim::RegisterClaim),
}

impl pallet_cf_signing::SigningContext<Runtime> for EthereumSigner {
	type Chain = cf_chains::Ethereum;
	type Payload = Vec<u8>;
	type Signature = eth::SchnorrSignature;
	type Callback = Call;

	fn get_payload(&self) -> Self::Payload {
		match self {
			eth::EthereumTransactions::RegisterClaim(ref tx) => {
				tx.sig_data.msg_hash
			},
		}
	}

	fn get_callback(&self, signature: Self::Signature) -> Self::Callback {
		match self {
			eth::EthereumTransactions::RegisterClaim(tx) => {
				pallet_cf_staking::Call::post_claim_signature(
					tx.node_id,
					tx.amount,
					signature
				)
			},
		}
	}
}

pub struct VaultKeyProvider;

impl KeyProvider for VaultKeyProvider {
	type KeyId = <Self as VaultsConfig>::PublicKey;

	fn current_key() -> Self::KeyId {
		todo!()
	}
}
