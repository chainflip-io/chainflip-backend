mod client;
pub mod crypto;
pub mod db;

#[cfg(test)]
mod tests;

pub use client::{
    start, KeyId, KeygenInfo, KeygenOutcome, MultisigEvent, MultisigInstruction, SchnorrSignature,
    SigningInfo, SigningOutcome,
};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Hash, Eq)]
pub struct MessageHash(pub [u8; 32]);

/// This is how we identify a signing ceremony (is it correct to assume
/// that SC won't try to sign the same message twice?)
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Hash, Eq)]
pub struct MessageInfo {
    pub hash: MessageHash,
    pub key_id: KeyId,
}
