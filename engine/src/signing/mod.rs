mod client;
pub mod crypto;

#[cfg(test)]
mod tests;

pub use client::{KeyId, KeygenInfo, MultisigClient, MultisigInstruction, SigningInfo};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Hash, Eq)]
pub struct MessageHash(pub Vec<u8>);

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Hash, Eq)]
pub struct MessageInfo {
    pub hash: MessageHash,
    pub key_id: KeyId,
}
