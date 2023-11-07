#![cfg(feature = "runtime-benchmarks")]

use sp_std::{vec, vec::Vec};

use crate::benchmarking_value::{BenchmarkValue, BenchmarkValueExtended};

use super::{
	api::{batch_transfer::BatchTransfer, BitcoinApi},
	deposit_address::BitcoinDepositChannel,
	AggKey, BitcoinFeeInfo, BitcoinFetchId, BitcoinFetchParams, BitcoinOutput, BitcoinTrackedData,
	BitcoinTransactionData, PreviousOrCurrent, ScriptPubkey, Signature, SigningPayload, Utxo,
	UtxoId,
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
		UtxoId { tx_id: [1u8; 32], vout: 1 }
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
			vec![BitcoinFetchParams::benchmark_value()],
			vec![BitcoinOutput {
				amount: Default::default(),
				script_pubkey: BenchmarkValue::benchmark_value(),
			}],
		))
	}
}

impl BenchmarkValue for BitcoinTrackedData {
	fn benchmark_value() -> Self {
		BitcoinTrackedData { btc_fee_info: BitcoinFeeInfo::new(4321) }
	}
}

impl BenchmarkValue for PreviousOrCurrent {
	fn benchmark_value() -> Self {
		Self::Current
	}
}

impl BenchmarkValueExtended for ScriptPubkey {
	fn benchmark_value_by_id(id: u8) -> Self {
		Self::Taproot([id; 32])
	}
}

impl BenchmarkValue for ScriptPubkey {
	fn benchmark_value() -> Self {
		Self::benchmark_value_by_id(0)
	}
}

impl BenchmarkValue for BitcoinDepositChannel {
	fn benchmark_value() -> Self {
		Self::benchmark_value_by_id(0)
	}
}

impl BenchmarkValueExtended for BitcoinDepositChannel {
	fn benchmark_value_by_id(id: u8) -> Self {
		BitcoinDepositChannel::new([id; 32], id as u32)
	}
}

impl BenchmarkValueExtended for UtxoId {
	fn benchmark_value_by_id(id: u8) -> Self {
		UtxoId { tx_id: [id; 32], vout: id as u32 }
	}
}

impl BenchmarkValueExtended for Utxo {
	fn benchmark_value_by_id(id: u8) -> Self {
		Utxo { id: UtxoId::benchmark_value_by_id(id), amount: 1_000 }
	}
}

impl BenchmarkValueExtended for BitcoinFetchParams {
	fn benchmark_value_by_id(id: u8) -> Self {
		Self {
			utxo: Utxo::benchmark_value_by_id(id),
			deposit_address: BitcoinDepositChannel::benchmark_value_by_id(id),
		}
	}
}

impl BenchmarkValue for BitcoinFetchParams {
	fn benchmark_value() -> Self {
		Self::benchmark_value_by_id(0)
	}
}
