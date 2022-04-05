// TODO: make it unnecessary to expose macros here
#[macro_use]
pub mod frost;
pub mod frost_stages;

use std::{sync::Arc, time::Instant};

use pallet_cf_vaults::CeremonyId;

use crate::multisig::{crypto::Rng, MessageHash};

use state_chain_runtime::AccountId;

use super::{ceremony_manager::CeremonyResultSender, common::KeygenResult, SchnorrSignature};

#[derive(Debug)]
pub struct PendingSigningRequest {
    pub ceremony_id: CeremonyId,
    pub signers: Vec<AccountId>,
    pub data: MessageHash,
    pub rng: Rng,
    pub should_expire_at: Instant,
    pub result_sender: CeremonyResultSender<SchnorrSignature>,
}

/// Data common for signing stages
#[derive(Clone)]
pub struct SigningStateCommonInfo {
    pub data: MessageHash,
    pub key: Arc<KeygenResult>,
}
