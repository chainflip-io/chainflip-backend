mod keygen_data;
mod keygen_frost;
mod keygen_stages;

use std::sync::Arc;

use pallet_cf_vaults::CeremonyId;
use serde::{Deserialize, Serialize};

pub use keygen_data::{
    BlameResponse6, Comm1, Complaints4, KeygenData, SecretShare3, VerifyBlameResponses7,
    VerifyComm2, VerifyComplaints5,
};

pub use keygen_stages::AwaitCommitments1;

use crate::p2p::AccountId;

dyn_clone::clone_trait_object!(CeremonyStage<Message = KeygenData, Result = KeygenResult>);

use super::{
    common::{CeremonyStage, KeygenResult, P2PSender, RawP2PSender},
    utils::PartyIdxMapping,
    EventSender, KeygenDataWrapped, MultisigMessage,
};

/// Information necessary for the multisig client to start a new keygen ceremony
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KeygenInfo {
    pub ceremony_id: CeremonyId,
    pub signers: Vec<AccountId>,
}

#[derive(Clone, Copy)]
pub struct KeygenOptions {
    /// This is intentionally private to ensure that the only
    /// way to unset this flag with via test-only constructor
    low_pubkey_only: bool,
}

impl Default for KeygenOptions {
    fn default() -> Self {
        Self {
            low_pubkey_only: true,
        }
    }
}

impl KeygenOptions {
    /// This should not be used in production as it could
    /// result in pubkeys incompatible with the KeyManager
    /// contract, but it is useful in tests that need to be
    /// deterministic and don't interact with the contract
    #[cfg(test)]
    pub fn allowing_high_pubkey() -> Self {
        Self {
            low_pubkey_only: false,
        }
    }
}

impl KeygenInfo {
    pub fn new(ceremony_id: CeremonyId, signers: Vec<AccountId>) -> Self {
        KeygenInfo {
            ceremony_id,
            signers,
        }
    }
}

/// Sending half of the channel that additionally maps signer_idx -> accountId
/// and wraps the binary data into the appropriate for keygen type
#[derive(Clone)]
pub struct KeygenP2PSender {
    ceremony_id: CeremonyId,
    sender: RawP2PSender,
}

impl KeygenP2PSender {
    pub fn new(
        validator_map: Arc<PartyIdxMapping>,
        sender: EventSender,
        ceremony_id: CeremonyId,
    ) -> Self {
        KeygenP2PSender {
            ceremony_id,
            sender: RawP2PSender::new(validator_map, sender),
        }
    }
}

impl P2PSender for KeygenP2PSender {
    type Data = KeygenData;

    fn send(&self, receiver_idx: usize, data: Self::Data) {
        let msg: MultisigMessage = KeygenDataWrapped::new(self.ceremony_id, data).into();
        let data = bincode::serialize(&msg).unwrap();
        self.sender.send(receiver_idx, data);
    }
}
