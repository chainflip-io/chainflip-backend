use sp_std::{vec, vec::Vec};

use crate::benchmarking_value::{BenchmarkValue, BenchmarkValueExtended};

use super::{
	api::{batch_transfer::BatchTransfer, BitcoinApi},
	AggKey, BitcoinFetchId, BitcoinOutput, BitcoinScriptBounded, BitcoinTrackedData,
	BitcoinTransactionData, PreviousOrCurrent, Signature, SigningPayload, Utxo, UtxoId,
};

impl BenchmarkValue for AggKey {
	fn benchmark_value() -> Self {
		AggKey { previous: None, current: [2u8; 32] }
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
		UtxoId { tx_hash: [1u8; 32], vout: 1 }
	}
}

// Bitcoin threshold signature
impl BenchmarkValue for Signature {
	fn benchmark_value() -> Self {
		[0xau8; 64]
	}
}

// Bitcoin payload
impl BenchmarkValue for SigningPayload {
	fn benchmark_value() -> Self {
		[1u8; 32]
	}
}

impl BenchmarkValue for BitcoinScriptBounded {
	fn benchmark_value() -> Self {
		BitcoinScriptBounded { data: [3u8; 100].to_vec().try_into().unwrap() }
	}
}

impl BenchmarkValueExtended for BitcoinScriptBounded {
	fn benchmark_value_by_id(id: u8) -> Self {
		BitcoinScriptBounded { data: [id; 100].to_vec().try_into().unwrap() }
	}
}

impl BenchmarkValue for BitcoinFetchId {
	fn benchmark_value() -> Self {
		Self(1)
	}
}

impl BenchmarkValueExtended for BitcoinFetchId {
	fn benchmark_value_by_id(id: u8) -> Self {
		Self(id.into())
	}
}

impl<E> BenchmarkValue for BitcoinApi<E> {
	fn benchmark_value() -> Self {
		BitcoinApi::BatchTransfer(BatchTransfer::new_unsigned(
			&BenchmarkValue::benchmark_value(),
			BenchmarkValue::benchmark_value(),
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

impl BenchmarkValue for BitcoinTrackedData {
	fn benchmark_value() -> Self {
		BitcoinTrackedData { block_height: 120, fee_rate_sats_per_byte: 4321 }
	}
}

impl BenchmarkValue for PreviousOrCurrent {
	fn benchmark_value() -> Self {
		Self::Current
	}
}
