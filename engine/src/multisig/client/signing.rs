// TODO: make it unnecessary to expose macros here
#[macro_use]
pub mod frost;
pub mod frost_stages;

use std::{sync::Arc, time::Instant};

use pallet_cf_vaults::CeremonyId;

use serde::{Deserialize, Serialize};

use crate::{
    constants::PENDING_SIGN_DURATION,
    multisig::{KeyId, MessageHash},
};

use state_chain_runtime::AccountId;

use super::common::KeygenResult;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SigningRequest {
    pub data: MessageHash,
    pub ceremony_id: CeremonyId,
    pub key_id: KeyId,
    pub signers: Vec<AccountId>,
}

impl SigningRequest {
    pub fn new(
        ceremony_id: CeremonyId,
        key_id: KeyId,
        data: MessageHash,
        signers: Vec<AccountId>,
    ) -> Self {
        SigningRequest {
            data,
            ceremony_id,
            key_id,
            signers,
        }
    }
}

/// A wrapper around SigningRequest that contains the timeout info for cleanup
#[derive(Debug)]
pub struct PendingSigningRequest {
    pub should_expire_at: Instant,
    pub signing_request: SigningRequest,
}

impl PendingSigningRequest {
    pub fn new(signing_request: SigningRequest) -> Self {
        PendingSigningRequest {
            should_expire_at: Instant::now() + PENDING_SIGN_DURATION,
            signing_request,
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
