use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_std::vec::Vec;

use crate::{
	btc::{AggKey, BitcoinCrypto, BitcoinFetchParams, BitcoinOutput, BitcoinTransaction},
	ApiCall, ChainCrypto,
};
use frame_support::sp_runtime::RuntimeDebug;

/// Represents all the arguments required to build the call to fetch assets for all given channel
/// ids.
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct BatchTransfer {
	/// The handler for creating and signing polkadot extrinsics
	pub bitcoin_transaction: BitcoinTransaction,
	pub change_utxo_key: [u8; 32],
}

impl BatchTransfer {
	pub fn new_unsigned(
		agg_key: &AggKey,
		change_utxo_key: [u8; 32],
		input_utxos: Vec<BitcoinFetchParams>,
		outputs: Vec<BitcoinOutput>,
	) -> Self {
		Self {
			bitcoin_transaction: BitcoinTransaction::create_new_unsigned(
				agg_key,
				input_utxos,
				outputs,
			),
			change_utxo_key,
		}
	}
}

impl ApiCall<BitcoinCrypto> for BatchTransfer {
	fn threshold_signature_payload(&self) -> <BitcoinCrypto as ChainCrypto>::Payload {
		self.bitcoin_transaction.get_signing_payloads()
	}

	fn signed(mut self, signatures: &<BitcoinCrypto as ChainCrypto>::ThresholdSignature) -> Self {
		self.bitcoin_transaction.add_signatures(signatures.clone());
		self
	}

	fn chain_encoded(&self) -> Vec<u8> {
		self.bitcoin_transaction.clone().finalize()
	}

	fn is_signed(&self) -> bool {
		self.bitcoin_transaction.is_signed()
	}

	fn transaction_out_id(&self) -> <BitcoinCrypto as ChainCrypto>::TransactionOutId {
		self.bitcoin_transaction.txid()
	}
}
