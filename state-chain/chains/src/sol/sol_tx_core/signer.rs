use crate::sol::{
	sol_tx_core::{signer::presigner::PresignerError, transaction::TransactionError},
	SolPubkey, SolSignature,
};
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
	/// `SolPubkey` if the implementor has none.
	fn pubkey(&self) -> SolPubkey {
		self.try_pubkey().unwrap_or_default()
	}
	/// Fallibly gets the implementor's public key
	fn try_pubkey(&self) -> Result<SolPubkey, SignerError>;
	/// Infallibly produces an Ed25519 signature over the provided `message`
	/// bytes. Returns the all-zeros `Signature` if signing is not possible.
	fn sign_message(&self, message: &[u8]) -> SolSignature {
		self.try_sign_message(message).unwrap_or_default()
	}
	/// Fallibly produces an Ed25519 signature over the provided `message` bytes.
	fn try_sign_message(&self, message: &[u8]) -> Result<SolSignature, SignerError>;
	/// Whether the implementation requires user interaction to sign
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

pub mod signers {
	use crate::sol::{
		sol_tx_core::signer::{Signer, SignerError},
		SolPubkey, SolSignature,
	};

	/// Convenience trait for working with mixed collections of `Signer`s
	pub trait Signers {
		fn pubkeys(&self) -> Vec<SolPubkey>;
		fn try_pubkeys(&self) -> Result<Vec<SolPubkey>, SignerError>;
		fn sign_message(&self, message: &[u8]) -> Vec<SolSignature>;
		fn try_sign_message(&self, message: &[u8]) -> Result<Vec<SolSignature>, SignerError>;
		fn is_interactive(&self) -> bool;
	}

	macro_rules! default_keypairs_impl {
		() => {
			fn pubkeys(&self) -> Vec<SolPubkey> {
				self.iter().map(|keypair| keypair.pubkey()).collect()
			}

			fn try_pubkeys(&self) -> Result<Vec<SolPubkey>, SignerError> {
				let mut pubkeys = Vec::new();
				for keypair in self.iter() {
					pubkeys.push(keypair.try_pubkey()?);
				}
				Ok(pubkeys)
			}

			fn sign_message(&self, message: &[u8]) -> Vec<SolSignature> {
				self.iter().map(|keypair| keypair.sign_message(message)).collect()
			}

			fn try_sign_message(&self, message: &[u8]) -> Result<Vec<SolSignature>, SignerError> {
				let mut signatures = Vec::new();
				for keypair in self.iter() {
					signatures.push(keypair.try_sign_message(message)?);
				}
				Ok(signatures)
			}

			fn is_interactive(&self) -> bool {
				self.iter().any(|s| s.is_interactive())
			}
		};
	}

	impl<T: Signer> Signers for [&T; 1] {
		default_keypairs_impl!();
	}
	impl<T: Signer> Signers for [&T; 2] {
		default_keypairs_impl!();
	}
}
