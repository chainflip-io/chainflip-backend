use crate::address::EncodedAddress;
use cf_primitives::{AffiliateAndFee, Asset, AssetAmount, ForeignChain};
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::ConstU32;
use sp_runtime::BoundedVec;
use sp_std::vec::Vec;

// The maximum length of data that can be encoded in a nulldata utxo
const MAX_NULLDATA_LENGTH: usize = 80;
const CURRENT_VERSION: u8 = 0;

#[derive(Clone, PartialEq, Debug, TypeInfo)]
pub struct UtxoEncodedData {
	pub output_asset: Asset,
	pub output_address: EncodedAddress,
	pub parameters: BtcCfParameters,
}

impl Encode for UtxoEncodedData {
	fn encode(&self) -> Vec<u8> {
		let mut r = Vec::with_capacity(MAX_NULLDATA_LENGTH);

		CURRENT_VERSION.encode_to(&mut r);

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
		let version = u8::decode(input)?;

		if version != CURRENT_VERSION {
			log::warn!(
				"Unexpected version of utxo encoding: {version} (expected: {CURRENT_VERSION})"
			);
			return Err("unexpected version".into());
		}

		let output_asset = Asset::decode(input)?;

		let output_address = match ForeignChain::from(output_asset) {
			ForeignChain::Ethereum => EncodedAddress::Eth(Decode::decode(input)?),
			ForeignChain::Polkadot => EncodedAddress::Dot(Decode::decode(input)?),
			ForeignChain::Bitcoin => EncodedAddress::Btc(Decode::decode(input)?),
			ForeignChain::Arbitrum => EncodedAddress::Arb(Decode::decode(input)?),
			ForeignChain::Solana => EncodedAddress::Sol(Decode::decode(input)?),
		};

		let parameters = BtcCfParameters::decode(input)?;

		Ok(UtxoEncodedData { output_asset, output_address, parameters })
	}
}

// We limit the number of affiliates in btc vault swaps to ensure that we
// can always encode them inside a UTXO
const MAX_AFFILIATES: u32 = 2;

// The encoding of these parameters is the same across chains
#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Debug)]
pub struct BtcCfParameters {
	// --- FoK fields (refund address is stored externally) ---
	pub retry_duration: u16,
	pub min_output_amount: AssetAmount,
	// --- DCA field ---
	pub number_of_chunks: u16,
	pub chunk_interval: u16,
	// --- Boost fields ---
	pub boost_fee: u8,
	// --- Broker fields ---
	// Primary's broker fee:
	pub broker_fee: u8,
	pub affiliates: BoundedVec<AffiliateAndFee, ConstU32<MAX_AFFILIATES>>,
}

pub fn encode_swap_params_in_nulldata_payload(params: UtxoEncodedData) -> Vec<u8> {
	params.encode()
}

#[cfg(test)]
mod tests {
	use sp_core::bounded_vec;

	use super::*;

	const MOCK_DOT_ADDRESS: [u8; 32] = [9u8; 32];

	#[test]
	fn check_utxo_encoding() {
		let mock_swap_params = UtxoEncodedData {
			output_asset: Asset::Dot,
			output_address: EncodedAddress::Dot(MOCK_DOT_ADDRESS),
			parameters: BtcCfParameters {
				retry_duration: 5,
				min_output_amount: u128::MAX,
				number_of_chunks: 0x0ffff,
				chunk_interval: 2,
				boost_fee: 5,
				broker_fee: 0xa,
				affiliates: bounded_vec![
					AffiliateAndFee { affiliate: 6.into(), fee: 7 },
					AffiliateAndFee { affiliate: 8.into(), fee: 9 }
				],
			},
		};
		// The following encoding is expected for MOCK_SWAP_PARAMS:
		// (not using "insta" because we want to be precise about how the data
		// is encoded exactly, rather than simply that the encoding doesn't change)
		let expected_encoding: Vec<u8> = []
			.into_iter()
			.chain([0x00]) // Version
			.chain([0x04]) // Asset
			.chain(MOCK_DOT_ADDRESS) // Polkadot address
			.chain([0x05, 0x00]) // Retry duration
			.chain([
				0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
				0xff, 0xff,
			]) // min output amount
			.chain([0xff, 0xff]) // Number of chunks
			.chain([0x02, 0x00]) // Chunk interval
			.chain([0x5]) // Boost fee
			.chain([0xa]) // Broker fee
			.chain([0x8, 0x6, 0x7, 0x8, 0x9]) // Affiliate fees (1 byte length + 2 bytes per affiliate)
			.collect();

		assert_eq!(mock_swap_params.encode(), expected_encoding);
		assert_eq!(expected_encoding.len(), 63);

		assert_eq!(UtxoEncodedData::decode(&mut expected_encoding.as_ref()), Ok(mock_swap_params));
	}
}
