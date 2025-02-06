use crate::{
	address::EncodedAddress,
	evm::{api::EvmCall, tokenizable::Tokenizable},
};
use cf_primitives::Asset;
use codec::{Decode, Encode};
use ethabi::Token;
use frame_support::sp_runtime::RuntimeDebug;
use scale_info::TypeInfo;
use sp_std::{vec, vec::Vec};

/// Represents all the arguments required to build the call to Vault's 'XSwapNative'
/// function.
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct XSwapNative {
	/// The destination chain according to Chainflip Protocol's nomenclature.
	dst_chain: u32,
	/// Bytes containing the destination address on the destination chain.
	dst_address: Vec<u8>,
	/// Destination token to be swapped to.
	dst_token: u32,
	/// Additional parameters to be passed to the Chainflip Protocol.
	cf_parameters: Vec<u8>,
}

impl XSwapNative {
	pub fn new(
		destination_address: EncodedAddress,
		destination_asset: Asset,
		cf_parameters: Vec<u8>,
	) -> Self {
		Self {
			dst_chain: destination_address.chain() as u32,
			dst_address: destination_address.inner_bytes().to_vec(),
			dst_token: destination_asset as u32,
			cf_parameters,
		}
	}
}

impl EvmCall for XSwapNative {
	const FUNCTION_NAME: &'static str = "xSwapNative";

	fn function_params() -> Vec<(&'static str, ethabi::ParamType)> {
		vec![
			("dstChain", u32::param_type()),
			("dstAddress", <Vec<u8>>::param_type()),
			("dstToken", u32::param_type()),
			("cfParameters", <Vec<u8>>::param_type()),
		]
	}

	fn function_call_args(&self) -> Vec<Token> {
		vec![
			self.dst_chain.tokenize(),
			self.dst_address.clone().tokenize(),
			self.dst_token.tokenize(),
			self.cf_parameters.clone().tokenize(),
		]
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::{
		eth::api::abi::load_abi,
		evm::api::{vault_swaps::test_utils::*, EvmTransactionBuilder},
	};
	use cf_primitives::ForeignChain;

	#[test]
	fn test_payload() {
		let dest_address = EncodedAddress::Dot([0xff; 32]);
		let dest_address_bytes = dest_address.inner_bytes().to_vec().clone();
		let dest_chain = ForeignChain::Polkadot as u32;
		let dest_asset = Asset::Dot;

		let eth_vault = load_abi("IVault");
		let function_reference = eth_vault.function("xSwapNative").unwrap();

		// Create the EVM call without replay protection and signer info.
		// It is expected for vault swap calls to be unsigned.
		let function_runtime = EvmTransactionBuilder::new_unsigned(
			Default::default(),
			super::XSwapNative::new(dest_address, dest_asset, dummy_cf_parameter(false)),
		);

		let runtime_payload = function_runtime.chain_encoded_payload();
		assert_eq!(
			// Our encoding:
			runtime_payload,
			// "Canonical" encoding based on the abi definition above and using the ethabi crate:
			function_reference
				.encode_input(&[
					dest_chain.tokenize(),
					dest_address_bytes.tokenize(),
					(dest_asset as u32).tokenize(),
					dummy_cf_parameter(false).tokenize(),
				])
				.unwrap()
		);

		// Check against harcoded tested payload
		assert_eq!(
			runtime_payload,
			[
				221, 104, 115, 69, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
				0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
				0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 128, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
				0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0,
				0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 192, 0, 0, 0,
				0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
				32, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
				255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0,
				0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
				0, 0, 104, 0, 1, 0, 0, 0, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240,
				240, 240, 240, 240, 240, 240, 240, 240, 240, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
				0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 10, 0, 0, 0, 5, 0, 0,
				0, 100, 242, 242, 242, 242, 242, 242, 242, 242, 242, 242, 242, 242, 242, 242, 242,
				242, 242, 242, 242, 242, 242, 242, 242, 242, 242, 242, 242, 242, 242, 242, 242,
				242, 1, 0, 4, 1, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
				0, 0, 0
			]
		);
	}
}
