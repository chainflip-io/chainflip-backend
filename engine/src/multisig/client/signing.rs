// TODO: make it unnecessary to expose macros here
#[macro_use]
pub mod frost;
pub mod frost_stages;

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use pallet_cf_vaults::CeremonyId;

use serde::{Deserialize, Serialize};

use crate::multisig::{KeyId, MessageHash};

use state_chain_runtime::AccountId;

use self::frost::SigningData;

use super::{
    common::{CeremonyStage, KeygenResult},
    SchnorrSignature,
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

const PENDING_SIGN_DURATION: Duration = Duration::from_secs(500); // TODO Look at this value

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

/// Data common for signing stages
#[derive(Clone)]
pub struct SigningStateCommonInfo {
    pub data: MessageHash,
    pub key: Arc<KeygenResult>,
}

dyn_clone::clone_trait_object!(CeremonyStage<Message = SigningData, Result = SchnorrSignature>);
