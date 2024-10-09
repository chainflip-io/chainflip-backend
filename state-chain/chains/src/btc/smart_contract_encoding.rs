use crate::ForeignChainAddress;
use cf_primitives::{Asset, AssetAmount};
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

use super::{BitcoinOp, BitcoinScript};

// The maximum length of data that can be encoded in a nulldata utxo
const MAX_NULLDATA_LENGTH: usize = 80;

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone)]
pub struct UtxoEncodedData {
	pub output_asset: Asset,
	pub output_address: ForeignChainAddress,
	pub parameters: SharedCfParameters,
}

// The encoding of these parameters is the same across chains
#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone)]
pub struct SharedCfParameters {
	// FoK fields (refund address is stored externally):
	pub retry_duration: u16,
	pub min_output_amount: AssetAmount,
	// DCA fields:
	pub number_of_chunks: u16,
	pub chunk_interval: u16,
	// Boost fields:
	pub boost_fee: u8,
}

#[allow(dead_code)]
pub fn encode_data_in_nulldata_utxo(data: &[u8]) -> Option<BitcoinScript> {
	if data.len() > MAX_NULLDATA_LENGTH {
		return None;
	}

	Some(BitcoinScript::new(&[
		BitcoinOp::Return,
		BitcoinOp::PushBytes { bytes: data.to_vec().try_into().expect("size checked just above") },
	]))
}

pub fn encode_swap_params_in_nulldata_utxo(params: UtxoEncodedData) -> Option<BitcoinScript> {
	encode_data_in_nulldata_utxo(&params.encode())
}

#[cfg(test)]
mod tests {
	use super::*;

	use crate::dot::PolkadotAccountId;
	use std::sync::LazyLock;

	const MOCK_DOT_ADDRESS: [u8; 32] = [9u8; 32];

	static MOCK_SWAP_PARAMS: LazyLock<UtxoEncodedData> = LazyLock::new(|| {
		let output_address = ForeignChainAddress::Dot(
			PolkadotAccountId::try_from(Vec::from(&MOCK_DOT_ADDRESS)).unwrap(),
		);

		UtxoEncodedData {
			output_asset: Asset::Btc,
			output_address,
			parameters: SharedCfParameters {
				retry_duration: 5,
				min_output_amount: u128::MAX,
				number_of_chunks: 0x0ffff,
				chunk_interval: 2,
				boost_fee: 5,
			},
		}
	});

	#[test]
	fn check_utxo_encoding() {
		// The following encoding is expected for MOCK_SWAP_PARAMS:
		// (not using "insta" because we want to be precise about how the data
		// is encoded exactly, rather than simply that the encoding doesn't change)
		let expected_encoding: Vec<u8> = [0x05] // Asset
			.into_iter()
			.chain([0x01]) // Tag for polkadot address
			.chain(MOCK_DOT_ADDRESS) // Polkadot address
			.chain([0x05, 0x00]) // Retry duration
			.chain([
				0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
				0xff, 0xff,
			]) // min output amount
			.chain([0xff, 0xff]) // Number of chunks
			.chain([0x02, 0x00]) // Chunk interval
			.chain([0x5]) // Boost fee
			.collect();

		assert_eq!(MOCK_SWAP_PARAMS.encode(), expected_encoding);
		assert_eq!(expected_encoding.len(), 57);
	}
}
