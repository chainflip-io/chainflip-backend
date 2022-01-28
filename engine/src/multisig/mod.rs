//! Multisig signing and keygen

/// Multisig client
mod client;
/// Provides cryptographic primitives used by the multisig client
mod crypto;
/// Storage for the keys
mod db;

#[cfg(test)]
mod tests;

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use serde::{Deserialize, Serialize};

use std::time::Duration;

use crate::{common, logging::COMPONENT_KEY, multisig_p2p::OutgoingMultisigStageMessages};
use futures::StreamExt;
use slog::o;
use state_chain_runtime::AccountId;

pub use client::{
    KeygenOptions, KeygenOutcome, MultisigClient, MultisigMessage, MultisigOutcome,
    SchnorrSignature, SigningOutcome,
};

pub use db::{KeyDB, PersistentKeyDB};

#[cfg(test)]
pub use db::KeyDBMock;

use self::client::KeygenResultInfo;
pub use self::client::{keygen::KeygenInfo, signing::SigningInfo};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Hash, Eq)]
pub struct MessageHash(pub [u8; 32]);

impl std::fmt::Display for MessageHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

/// Public key compressed (33 bytes = 32 bytes + a y parity byte)
#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Debug, Clone)]
pub struct KeyId(pub Vec<u8>); // TODO: Use [u8; 33] not a Vec

impl std::fmt::Display for KeyId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(&self.0))
    }
}

#[derive(Debug)]
pub enum MultisigInstruction {
    Keygen((KeygenInfo, KeygenOptions)),
    Sign((SigningInfo, KeygenResultInfo)),
}
