// TODO: make it unnecessary to expose macros here
#[macro_use]
pub mod frost;
mod frost_stages;
mod signing_manager;
mod signing_state;

use std::time::{Duration, Instant};

pub use frost::SigningDataWrapped;
use pallet_cf_vaults::CeremonyId;
pub use signing_manager::SigningManager;

use serde::{Deserialize, Serialize};

use crate::{
    multisig::{KeyId, MessageHash},
    p2p::AccountId,
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
