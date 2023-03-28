use cf_primitives::MAX_BTC_ADDRESS_LENGTH;
use frame_support::BoundedVec;
use sp_std::{vec, vec::Vec};

use crate::{
	address::{BitcoinAddress, BitcoinAddressData, BitcoinAddressFor, BitcoinAddressSeed},
	benchmarking_value::BenchmarkValue,
};

use super::{
	api::{batch_transfer::BatchTransfer, BitcoinApi},
	AggKey, BitcoinFetchId, BitcoinNetwork, BitcoinOutput, BitcoinTransactionData, Signature,
	SigningPayload, Utxo, UtxoId,
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
		UtxoId { tx_hash: [1u8; 32], vout: 1, pubkey_x: [2u8; 32], salt: 0 }
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

// Bitcoin address
impl BenchmarkValue for BitcoinAddress {
	fn benchmark_value() -> Self {
		BoundedVec::try_from([1u8; MAX_BTC_ADDRESS_LENGTH as usize].to_vec())
			.expect("we created a vec that is in the bounds of bounded vec")
	}
}

impl BenchmarkValue for BitcoinAddressData {
	fn benchmark_value() -> Self {
		BitcoinAddressData {
			address_for: BitcoinAddressFor::Ingress(BitcoinAddressSeed {
				pubkey_x: [2u8; 32],
				salt: 7,
			}),
			network: BitcoinNetwork::Testnet,
		}
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
