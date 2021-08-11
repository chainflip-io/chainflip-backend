mod client;
pub mod crypto;
pub mod db;

#[cfg(test)]
mod tests;

pub use client::{
    start, KeyId, KeygenInfo, KeygenOutcome, KeygenSuccess, MultisigEvent, MultisigInstruction,
    SigningInfo, SigningOutcome, SigningSuccess, SchnorrSignature
};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Hash, Eq)]
pub struct MessageHash(pub [u8; 32]);

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Hash, Eq)]
pub struct MessageInfo {
    pub hash: MessageHash,
    pub key_id: KeyId,
}
