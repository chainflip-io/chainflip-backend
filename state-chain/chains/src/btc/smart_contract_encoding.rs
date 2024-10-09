use crate::address::EncodedAddress;
use cf_primitives::{Asset, AssetAmount};
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

use super::{BitcoinOp, BitcoinScript};

// The maximum length of data that can be encoded in a nulldata utxo
const MAX_NULLDATA_LENGTH: usize = 80;

#[derive(Clone, PartialEq, Debug, TypeInfo)]
pub struct UtxoEncodedData {
	pub output_asset: Asset,
	pub output_address: EncodedAddress,
	pub parameters: SharedCfParameters,
}

impl Encode for UtxoEncodedData {
	fn encode(&self) -> Vec<u8> {
		let mut r = Vec::with_capacity(MAX_NULLDATA_LENGTH);

		self.output_asset.encode_to(&mut r);

		// Note that we don't encode the variant since we know the
		// asset from reading the previous field:
		match &self.output_address {
			EncodedAddress::Eth(inner) => inner.encode_to(&mut r),
			EncodedAddress::Dot(inner) => inner.encode_to(&mut r),
			EncodedAddress::Btc(inner) => inner.encode_to(&mut r),
			EncodedAddress::Arb(inner) => inner.encode_to(&mut r),
			EncodedAddress::Sol(inner) => inner.encode_to(&mut r),
		}

		self.parameters.encode_to(&mut r);

		r
	}
}

impl Decode for UtxoEncodedData {
	fn decode<I: codec::Input>(input: &mut I) -> Result<Self, codec::Error> {
		let output_asset = Asset::decode(input)?;

		let output_address = match output_asset {
			Asset::Eth | Asset::Flip | Asset::Usdc | Asset::Usdt =>
				EncodedAddress::Eth(<[u8; 20]>::decode(input)?),
			Asset::Dot => EncodedAddress::Dot(<[u8; 32]>::decode(input)?),
			Asset::Btc => EncodedAddress::Btc(Vec::<u8>::decode(input)?),
			Asset::ArbEth | Asset::ArbUsdc => EncodedAddress::Arb(<[u8; 20]>::decode(input)?),
			Asset::Sol | Asset::SolUsdc =>
				EncodedAddress::Sol(<[u8; sol_prim::consts::SOLANA_ADDRESS_LEN]>::decode(input)?),
		};

		let parameters = SharedCfParameters::decode(input)?;

		Ok(UtxoEncodedData { output_asset, output_address, parameters })
	}
}

// The encoding of these parameters is the same across chains
#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Debug)]
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

	use core::ops::Deref;
	use std::sync::LazyLock;

	const MOCK_DOT_ADDRESS: [u8; 32] = [9u8; 32];

	static MOCK_SWAP_PARAMS: LazyLock<UtxoEncodedData> = LazyLock::new(|| {
		let output_address = EncodedAddress::Dot(MOCK_DOT_ADDRESS.clone());

		UtxoEncodedData {
			output_asset: Asset::Dot,
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
		let expected_encoding: Vec<u8> = [0x04] // Asset
			.into_iter()
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
		assert_eq!(expected_encoding.len(), 56);

		assert_eq!(
			UtxoEncodedData::decode(&mut expected_encoding.as_ref()).as_ref(),
			Ok(MOCK_SWAP_PARAMS.deref())
		);
	}
}
