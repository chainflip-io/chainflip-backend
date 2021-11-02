// TODO: make it unnecessary to expose macros here
#[macro_use]
pub mod frost;
pub mod frost_stages;

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

pub use frost::SigningDataWrapped;
use pallet_cf_vaults::CeremonyId;

use serde::{Deserialize, Serialize};

use crate::{
    multisig::{client::MultisigMessage, KeyId, MessageHash},
    p2p::AccountId,
};

use self::frost::SigningData;

use super::{
    common::{CeremonyStage, KeygenResult, P2PSender, RawP2PSender},
    utils::PartyIdxMapping,
    EventSender, SchnorrSignature,
};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SigningInfo {
    pub data: MessageHash,
    pub ceremony_id: CeremonyId,
    pub key_id: KeyId,
    pub signers: Vec<AccountId>,
}

impl SigningInfo {
    pub fn new(
        ceremony_id: CeremonyId,
        key_id: KeyId,
        data: MessageHash,
        signers: Vec<AccountId>,
    ) -> Self {
        SigningInfo {
            data,
            ceremony_id,
            key_id,
            signers,
        }
    }
}

const PENDING_SIGN_DURATION: Duration = Duration::from_secs(120);

/// A wrapper around SigningInfo that contains the timeout info for cleanup
#[derive(Clone, Debug)]
pub struct PendingSigningInfo {
    pub should_expire_at: Instant,
    pub signing_info: SigningInfo,
}

impl PendingSigningInfo {
    pub fn new(signing_info: SigningInfo) -> Self {
        PendingSigningInfo {
            should_expire_at: Instant::now() + PENDING_SIGN_DURATION,
            signing_info,
        }
    }

    #[cfg(test)]
    pub fn set_expiry_time(&mut self, expiry_time: Instant) {
        self.should_expire_at = expiry_time;
    }
}

/// Sending half of the channel that additionally maps signer_idx -> accountId
/// and wraps the binary data into the appropriate for signing type
#[derive(Clone)]
pub struct SigningP2PSender {
    ceremony_id: CeremonyId,
    sender: RawP2PSender,
}

impl SigningP2PSender {
    pub fn new(
        validator_map: Arc<PartyIdxMapping>,
        sender: EventSender,
        ceremony_id: CeremonyId,
    ) -> Self {
        SigningP2PSender {
            ceremony_id,
            sender: RawP2PSender::new(validator_map, sender),
        }
    }
}

impl P2PSender for SigningP2PSender {
    type Data = SigningData;

    fn send(&self, receiver_idx: usize, data: Self::Data) {
        let msg: MultisigMessage = SigningDataWrapped::new(data, self.ceremony_id).into();
        let data = bincode::serialize(&msg)
            .unwrap_or_else(|e| panic!("Could not serialise MultisigMessage: {:?}: {}", msg, e));
        self.sender.send(receiver_idx, data);
    }
}

/// Data common for signing stages
#[derive(Clone)]
pub struct SigningStateCommonInfo {
    pub data: MessageHash,
    pub key: Arc<KeygenResult>,
}

dyn_clone::clone_trait_object!(CeremonyStage<Message = SigningData, Result = SchnorrSignature>);
