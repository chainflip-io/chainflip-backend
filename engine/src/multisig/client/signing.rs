// TODO: make it unnecessary to expose macros here
#[macro_use]
pub mod frost;
pub mod frost_stages;

use std::{sync::Arc, time::Instant};

use pallet_cf_vaults::CeremonyId;
use tokio::sync::oneshot;

use crate::{
    constants::PENDING_SIGN_DURATION,
    multisig::{KeyId, MessageHash},
};

use state_chain_runtime::AccountId;

use super::{common::KeygenResult, CeremonyError, SchnorrSignature};

#[derive(Debug)]
pub struct SigningInfo {
    pub data: MessageHash,
    pub ceremony_id: CeremonyId,
    pub key_id: KeyId,
    pub signers: Vec<AccountId>,
    pub result_sender: oneshot::Sender<Result<SchnorrSignature, CeremonyError>>,
}

impl SigningInfo {
    pub fn new(
        ceremony_id: CeremonyId,
        key_id: KeyId,
        data: MessageHash,
        signers: Vec<AccountId>,
        result_sender: oneshot::Sender<Result<SchnorrSignature, CeremonyError>>,
    ) -> Self {
        SigningInfo {
            data,
            ceremony_id,
            key_id,
            signers,
            result_sender,
        }
    }
}

/// A wrapper around SigningInfo that contains the timeout info for cleanup
#[derive(Debug)]
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

/// Data common for signing stages
#[derive(Clone)]
pub struct SigningStateCommonInfo {
    pub data: MessageHash,
    pub key: Arc<KeygenResult>,
}
