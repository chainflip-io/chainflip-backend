//! Multisig signing and keygen
pub use crypto::{
	bitcoin, eth, polkadot, ChainTag, CryptoScheme, Rng, SignatureToThresholdSignature,
	CHAIN_TAG_SIZE,
};

pub use client::{MultisigClient, MultisigMessage};

/// Multisig client
pub mod client;
/// Provides cryptographic primitives used by the multisig client
mod crypto;

pub use crate as multisig;

pub mod p2p {
	use cf_primitives::AccountId;

	pub type ProtocolVersion = u16;

	/// Currently active wire protocol version
	pub const CURRENT_PROTOCOL_VERSION: ProtocolVersion = 1;

	// TODO: Consider if this should be removed, particularly once we no longer use Substrate for
	// peering
	#[derive(Debug, PartialEq, Eq)]
	pub enum OutgoingMultisigStageMessages {
		Broadcast(Vec<AccountId>, Vec<u8>),
		Private(Vec<(AccountId, Vec<u8>)>),
	}

	#[derive(Debug)]
	pub struct VersionedCeremonyMessage {
		pub version: ProtocolVersion,
		pub payload: Vec<u8>,
	}
}
