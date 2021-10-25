mod keygen_data;
mod keygen_frost;
mod keygen_stages;
mod keygen_state;

use pallet_cf_vaults::CeremonyId;
use serde::{Deserialize, Serialize};

pub use keygen_data::{
    Comm1, Complaints4, KeygenData, SecretShare3, VerifyComm2, VerifyComplaints5,
};

pub use keygen_stages::AwaitCommitments1;
pub use keygen_state::{KeygenP2PSender, KeygenState};

use crate::p2p::AccountId;

/// Information necessary for the multisig client to start a new keygen ceremony
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KeygenInfo {
    pub ceremony_id: CeremonyId,
    pub signers: Vec<AccountId>,
}

impl KeygenInfo {
    pub fn new(ceremony_id: CeremonyId, signers: Vec<AccountId>) -> Self {
        KeygenInfo {
            ceremony_id,
            signers,
        }
    }
}
