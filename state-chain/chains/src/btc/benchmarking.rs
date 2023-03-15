use cf_primitives::MAX_BTC_ADDRESS_LENGTH;
use frame_support::BoundedVec;
use sp_std::{vec, vec::Vec};

use crate::BenchmarkValue;

use super::{
	api::{batch_transfer::BatchTransfer, BitcoinApi},
	AggKey, BitcoinFetchId, BitcoinOutput, BitcoinPayload, BitcoinTransactionData, BtcAddress,
	Signature, Utxo, UtxoId,
};

impl BenchmarkValue for AggKey {
	fn benchmark_value() -> Self {
		AggKey([1u8; 32])
	}
}

impl BenchmarkValue for BitcoinTransactionData {
	fn benchmark_value() -> Self {
		Self { encoded_transaction: vec![1u8; 100] }
	}
}

impl<T: BenchmarkValue> BenchmarkValue for Vec<T> {
	fn benchmark_value() -> Self {
		vec![T::benchmark_value()]
	}
}

impl BenchmarkValue for UtxoId {
	fn benchmark_value() -> Self {
		UtxoId { tx_hash: [1u8; 32], vout_index: 1, pubkey_x: [2u8; 32], salt: 0 }
	}
}

// Bitcoin threshold signature
impl BenchmarkValue for Signature {
	fn benchmark_value() -> Self {
		[0xau8; 64]
	}
}

// Bitcoin payload
impl BenchmarkValue for BitcoinPayload {
	fn benchmark_value() -> Self {
		Self { payload: [1u8; 32], key_to_be_signed_with: AggKey([2u8; 32]) } // revisit this later, this
		                                                              // may not be
		                                                              // correct
	}
}

// Bitcoin address
impl BenchmarkValue for BtcAddress {
	fn benchmark_value() -> Self {
		BoundedVec::try_from([1u8; MAX_BTC_ADDRESS_LENGTH].to_vec())
			.expect("we created a vec that is in the bounds of bounded vec")
	}
}

impl BenchmarkValue for BitcoinFetchId {
	fn benchmark_value() -> Self {
		Self(1)
	}
}

impl<E> BenchmarkValue for BitcoinApi<E> {
	fn benchmark_value() -> Self {
		BitcoinApi::BatchTransfer(BatchTransfer::new_unsigned(
			vec![Utxo {
				amount: Default::default(),
				txid: Default::default(),
				vout: Default::default(),
				pubkey_x: Default::default(),
				salt: Default::default(),
			}],
			vec![BitcoinOutput { amount: Default::default(), script_pubkey: Default::default() }],
		))
	}
}
