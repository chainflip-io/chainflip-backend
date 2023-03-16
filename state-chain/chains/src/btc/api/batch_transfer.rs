use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_std::{vec, vec::Vec};

use crate::btc::{Bitcoin, BitcoinOutput, BitcoinTransaction, Utxo};

use crate::{ApiCall, ChainCrypto};

use sp_runtime::RuntimeDebug;

/// Represents all the arguments required to build the call to fetch assets for all given intent
/// ids.
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct BatchTransfer {
	/// The handler for creating and signing polkadot extrinsics
	pub bitcoin_transaction: BitcoinTransaction,
	/// The list of all inbound deposits that are to be fetched in this batch call.
	pub input_utxos: Vec<Utxo>,
	/// The list of all outbound transfers that are to be executed in this call.
	pub outputs: Vec<BitcoinOutput>,
}

impl BatchTransfer {
	pub fn new_unsigned(input_utxos: Vec<Utxo>, outputs: Vec<BitcoinOutput>) -> Self {
		Self {
			bitcoin_transaction: BitcoinTransaction::create_new_unsigned(
				input_utxos.clone(),
				outputs.clone(),
			),
			input_utxos,
			outputs,
		}
	}
}

impl ApiCall<Bitcoin> for BatchTransfer {
	fn threshold_signature_payload(&self) -> <Bitcoin as ChainCrypto>::Payload {
		let mut payloads = vec![];
		for i in 0..self.input_utxos.len() {
			payloads.push(self.bitcoin_transaction.get_signing_payload(i as u32))
		}
		payloads
	}

	fn signed(mut self, signatures: &<Bitcoin as ChainCrypto>::ThresholdSignature) -> Self {
		for (i, signature) in signatures.iter().enumerate() {
			self.bitcoin_transaction.add_signature(i as u32, *signature);
		}
		self
	}

	fn chain_encoded(&self) -> Vec<u8> {
		self.bitcoin_transaction.clone().finalize()
	}

	fn is_signed(&self) -> bool {
		self.bitcoin_transaction.is_signed()
	}
}
