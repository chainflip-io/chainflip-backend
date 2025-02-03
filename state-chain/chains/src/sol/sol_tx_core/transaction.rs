use crate::sol::{
	sol_tx_core::{
		compile_instructions, short_vec, CompiledInstruction, CompiledKeys, Hash, Instruction,
		MessageHeader, Pubkey, RawSignature,
	},
	SolSignature,
};
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
#[cfg(any(test, feature = "runtime-integration-tests"))]
use sol_prim::errors::TransactionError;
use sp_std::{vec, vec::Vec};

#[cfg(any(test, feature = "runtime-integration-tests"))]
use crate::sol::sol_tx_core::signer::{Signer, SignerError, TestSigners};

#[cfg(test)]
use crate::sol::sol_tx_core::program_instructions;

pub mod versioned_v0 {}

pub mod legacy {
	use super::*;

	/// A Solana transaction message (legacy).
	///
	/// See the [`message`] module documentation for further description.
	///
	/// [`message`]: crate::message
	///
	/// Some constructors accept an optional `payer`, the account responsible for
	/// paying the cost of executing a transaction. In most cases, callers should
	/// specify the payer explicitly in these constructors. In some cases though,
	/// the caller is not _required_ to specify the payer, but is still allowed to:
	/// in the `Message` structure, the first account is always the fee-payer, so if
	/// the caller has knowledge that the first account of the constructed
	/// transaction's `Message` is both a signer and the expected fee-payer, then
	/// redundantly specifying the fee-payer is not strictly required.
	// NOTE: Serialization-related changes must be paired with the custom serialization
	// for versioned messages in the `RemainingMessage` struct.
	#[derive(
		Encode, Decode, TypeInfo, Serialize, Deserialize, Default, Debug, PartialEq, Eq, Clone,
	)]
	#[serde(rename_all = "camelCase")]
	pub struct LegacyMessage {
		/// The message header, identifying signed and read-only `account_keys`.
		// NOTE: Serialization-related changes must be paired with the direct read at sigverify.
		pub header: MessageHeader,

		/// All the account keys used by this transaction.
		#[serde(with = "short_vec")]
		pub account_keys: Vec<Pubkey>,

		/// The id of a recent ledger entry.
		pub recent_blockhash: Hash,

		/// Programs that will be executed in sequence and committed in one atomic transaction if
		/// all succeed.
		#[serde(with = "short_vec")]
		pub instructions: Vec<CompiledInstruction>,
	}

	impl LegacyMessage {
		pub fn new_with_blockhash(
			instructions: &[Instruction],
			payer: Option<&Pubkey>,
			blockhash: &Hash,
		) -> Self {
			let compiled_keys = CompiledKeys::compile(instructions, payer.cloned());
			let (header, account_keys) = compiled_keys
				.try_into_message_components()
				.expect("overflow when compiling message keys");
			let instructions = compile_instructions(instructions, &account_keys);
			Self::new_with_compiled_instructions(
				header.num_required_signatures,
				header.num_readonly_signed_accounts,
				header.num_readonly_unsigned_accounts,
				account_keys,
				*blockhash,
				instructions,
			)
		}

		pub fn new(instructions: &[Instruction], payer: Option<&Pubkey>) -> Self {
			Self::new_with_blockhash(instructions, payer, &Hash::default())
		}

		#[cfg(test)]
		pub fn new_with_nonce(
			mut instructions: Vec<Instruction>,
			payer: Option<&Pubkey>,
			nonce_account_pubkey: &Pubkey,
			nonce_authority_pubkey: &Pubkey,
		) -> Self {
			let nonce_ix = program_instructions::SystemProgramInstruction::advance_nonce_account(
				nonce_account_pubkey,
				nonce_authority_pubkey,
			);
			instructions.insert(0, nonce_ix);
			Self::new(&instructions, payer)
		}

		fn new_with_compiled_instructions(
			num_required_signatures: u8,
			num_readonly_signed_accounts: u8,
			num_readonly_unsigned_accounts: u8,
			account_keys: Vec<Pubkey>,
			recent_blockhash: Hash,
			instructions: Vec<CompiledInstruction>,
		) -> Self {
			Self {
				header: MessageHeader {
					num_required_signatures,
					num_readonly_signed_accounts,
					num_readonly_unsigned_accounts,
				},
				account_keys,
				recent_blockhash,
				instructions,
			}
		}

		pub fn serialize(&self) -> Vec<u8> {
			bincode::serde::encode_to_vec(self, bincode::config::legacy()).unwrap()
		}
	}

	/// An atomically-committed sequence of instructions.
	///
	/// While [`Instruction`]s are the basic unit of computation in Solana,
	/// they are submitted by clients in [`Transaction`]s containing one or
	/// more instructions, and signed by one or more [`Signer`]s.
	///
	/// [`Signer`]: crate::signer::Signer
	///
	/// See the [module documentation] for more details about transactions.
	///
	/// [module documentation]: self
	///
	/// Some constructors accept an optional `payer`, the account responsible for
	/// paying the cost of executing a transaction. In most cases, callers should
	/// specify the payer explicitly in these constructors. In some cases though,
	/// the caller is not _required_ to specify the payer, but is still allowed to:
	/// in the [`Message`] structure, the first account is always the fee-payer, so
	/// if the caller has knowledge that the first account of the constructed
	/// transaction's `Message` is both a signer and the expected fee-payer, then
	/// redundantly specifying the fee-payer is not strictly required.
	#[derive(
		Encode, Decode, TypeInfo, Debug, PartialEq, Default, Eq, Clone, Serialize, Deserialize,
	)]
	pub struct LegacyTransaction {
		/// A set of signatures of a serialized [`Message`], signed by the first
		/// keys of the `Message`'s [`account_keys`], where the number of signatures
		/// is equal to [`num_required_signatures`] of the `Message`'s
		/// [`MessageHeader`].
		///
		/// [`account_keys`]: Message::account_keys
		/// [`MessageHeader`]: crate::message::MessageHeader
		/// [`num_required_signatures`]: crate::message::MessageHeader::num_required_signatures
		// NOTE: Serialization-related changes must be paired with the direct read at sigverify.
		#[serde(with = "short_vec")]
		pub signatures: Vec<SolSignature>,

		/// The message to sign.
		pub message: LegacyMessage,
	}

	impl LegacyTransaction {
		pub fn new_unsigned(message: LegacyMessage) -> Self {
			Self {
				signatures: vec![
					SolSignature::default();
					message.header.num_required_signatures as usize
				],
				message,
			}
		}

		#[cfg(any(test, feature = "runtime-integration-tests"))]
		pub fn new_with_payer(instructions: &[Instruction], payer: Option<&Pubkey>) -> Self {
			let message = LegacyMessage::new(instructions, payer);
			Self::new_unsigned(message)
		}

		#[cfg(any(test, feature = "runtime-integration-tests"))]
		pub fn sign<S: Signer>(&mut self, signers: TestSigners<S>, recent_blockhash: Hash) {
			if let Err(e) = self.try_sign(signers, recent_blockhash) {
				panic!("Transaction::sign failed with error {e:?}");
			}
		}

		#[cfg(any(test, feature = "runtime-integration-tests"))]
		pub fn try_sign<S: Signer>(
			&mut self,
			signers: TestSigners<S>,
			recent_blockhash: Hash,
		) -> Result<(), SignerError> {
			self.try_partial_sign(signers, recent_blockhash)?;

			if !self.is_signed() {
				Err(SignerError::NotEnoughSigners)
			} else {
				Ok(())
			}
		}

		#[cfg(any(test, feature = "runtime-integration-tests"))]
		pub fn try_partial_sign<S: Signer>(
			&mut self,
			signers: TestSigners<S>,
			recent_blockhash: Hash,
		) -> Result<(), SignerError> {
			let positions = self.get_signing_keypair_positions(signers.pubkeys())?;
			if positions.iter().any(|pos| pos.is_none()) {
				return Err(SignerError::KeypairPubkeyMismatch)
			}
			let positions: Vec<usize> = positions.iter().map(|pos| pos.unwrap()).collect();
			self.try_partial_sign_unchecked(signers, positions, recent_blockhash)
		}

		#[cfg(any(test, feature = "runtime-integration-tests"))]
		pub fn try_partial_sign_unchecked<S: Signer>(
			&mut self,
			signers: TestSigners<S>,
			positions: Vec<usize>,
			recent_blockhash: Hash,
		) -> Result<(), SignerError> {
			// if you change the blockhash, you're re-signing...
			if recent_blockhash != self.message.recent_blockhash {
				self.message.recent_blockhash = recent_blockhash;
				self.signatures
					.iter_mut()
					.for_each(|signature| *signature = SolSignature::default());
			}

			let signatures = signers.try_sign_message(&self.message_data())?;
			for i in 0..positions.len() {
				self.signatures[positions[i]] = signatures[i];
			}
			Ok(())
		}

		#[cfg(any(test, feature = "runtime-integration-tests"))]
		pub fn get_signing_keypair_positions(
			&self,
			pubkeys: Vec<Pubkey>,
		) -> Result<Vec<Option<usize>>, TransactionError> {
			if self.message.account_keys.len() <
				self.message.header.num_required_signatures as usize
			{
				return Err(TransactionError::InvalidAccountIndex)
			}
			let signed_keys =
				&self.message.account_keys[0..self.message.header.num_required_signatures as usize];

			Ok(pubkeys
				.iter()
				.map(|pubkey| signed_keys.iter().position(|x| x == pubkey))
				.collect())
		}

		pub fn is_signed(&self) -> bool {
			self.signatures.iter().all(|signature| *signature != SolSignature::default())
		}

		/// Return the message containing all data that should be signed.
		pub fn message(&self) -> &LegacyMessage {
			&self.message
		}

		/// Return the serialized message data to sign.
		pub fn message_data(&self) -> Vec<u8> {
			self.message().serialize()
		}

		/// Due to different Serialization between SolSignature and Solana native Signature type,
		/// the SolSignatures needs to be converted into the RawSignature type before the
		/// transaction is serialized as whole.
		pub fn finalize_and_serialize(self) -> Result<Vec<u8>, bincode::error::EncodeError> {
			bincode::serde::encode_to_vec(
				LegacyRawTransaction::from(self),
				bincode::config::legacy(),
			)
		}
	}

	/// Internal raw transaction type used for correct Serialization and Encoding
	#[derive(Debug, PartialEq, Default, Eq, Clone, Serialize, Deserialize)]
	struct LegacyRawTransaction {
		#[serde(with = "short_vec")]
		pub signatures: Vec<RawSignature>,
		pub message: LegacyMessage,
	}

	impl From<LegacyTransaction> for LegacyRawTransaction {
		fn from(from: LegacyTransaction) -> Self {
			Self {
				signatures: from.signatures.into_iter().map(RawSignature::from).collect(),
				message: from.message,
			}
		}
	}
}
