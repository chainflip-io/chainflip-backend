use crate::{errors::TransactionError, signer::presigner::PresignerError, Pubkey, Signature};

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SignerError {
	#[error("keypair-pubkey mismatch")]
	KeypairPubkeyMismatch,

	#[error("not enough signers")]
	NotEnoughSigners,

	#[error("transaction error")]
	TransactionError(#[from] TransactionError),

	#[error("custom error: {0}")]
	Custom(String),

	// Presigner-specific Errors
	#[error("presigner error")]
	PresignerError(#[from] PresignerError),

	// Remote Keypair-specific Errors
	#[error("connection error: {0}")]
	Connection(String),

	#[error("invalid input: {0}")]
	InvalidInput(String),

	#[error("no device found")]
	NoDeviceFound,

	#[error("{0}")]
	Protocol(String),

	#[error("{0}")]
	UserCancel(String),

	#[error("too many signers")]
	TooManySigners,
}

/// The `Signer` trait declares operations that all digital signature providers
/// must support. It is the primary interface by which signers are specified in
/// `Transaction` signing interfaces
pub trait Signer {
	/// Infallibly gets the implementor's public key. Returns the all-zeros
	/// `Pubkey` if the implementor has none.
	fn pubkey(&self) -> Pubkey {
		self.try_pubkey().unwrap_or_default()
	}
	/// Fallibly gets the implementor's public key
	fn try_pubkey(&self) -> Result<Pubkey, SignerError>;
	/// Infallibly produces an Ed25519 signature over the provided `message`
	/// bytes. Returns the all-zeros `Signature` if signing is not possible.
	fn sign_message(&self, message: &[u8]) -> Signature {
		self.try_sign_message(message).unwrap_or_default()
	}
	/// Fallibly produces an Ed25519 signature over the provided `message` bytes.
	fn try_sign_message(&self, message: &[u8]) -> Result<Signature, SignerError>;
	/// Whether the implementation requires user interaction to sign
	fn is_interactive(&self) -> bool;
}

/// Convenience trait for working with mixed collections of `Signer`s
pub trait Signers {
	fn pubkeys(&self) -> Vec<Pubkey>;
	fn try_pubkeys(&self) -> Result<Vec<Pubkey>, SignerError>;
	fn sign_message(&self, message: &[u8]) -> Vec<Signature>;
	fn try_sign_message(&self, message: &[u8]) -> Result<Vec<Signature>, SignerError>;
	fn is_interactive(&self) -> bool;
}

pub mod presigner {
	use thiserror::Error;

	#[derive(Debug, Error, PartialEq, Eq)]
	pub enum PresignerError {
		#[error("pre-generated signature cannot verify data")]
		VerificationFailure,
	}
}

pub struct TestSigners<S>(pub Vec<S>);
impl<S: Signer> TestSigners<S> {
	pub fn pubkeys(&self) -> Vec<Pubkey> {
		self.0.iter().map(|keypair| keypair.pubkey()).collect()
	}

	pub fn try_pubkeys(&self) -> Result<Vec<Pubkey>, SignerError> {
		let mut pubkeys = Vec::new();
		for keypair in self.0.iter() {
			pubkeys.push(keypair.try_pubkey()?);
		}
		Ok(pubkeys)
	}

	pub fn sign_message(&self, message: &[u8]) -> Vec<Signature> {
		self.0.iter().map(|keypair| keypair.sign_message(message)).collect()
	}

	pub fn try_sign_message(&self, message: &[u8]) -> Result<Vec<Signature>, SignerError> {
		let mut signatures = Vec::new();
		for keypair in self.0.iter() {
			signatures.push(keypair.try_sign_message(message)?);
		}
		Ok(signatures)
	}

	pub fn is_interactive(&self) -> bool {
		self.0.iter().any(|s| s.is_interactive())
	}
}

impl<S> From<Vec<S>> for TestSigners<S> {
	fn from(s: Vec<S>) -> Self {
		Self(s)
	}
}
