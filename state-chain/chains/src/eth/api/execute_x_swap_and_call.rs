use cf_primitives::{ForeignChainAddress, ETHEREUM_CHAIN_ID, POLKADOT_CHAIN_ID};
use codec::{Decode, Encode};
use ethabi::{ParamType, Token};
use scale_info::TypeInfo;
use sp_std::{vec, vec::Vec};

use crate::{
	eth::{api::all_batch::EncodableTransferAssetParams, Ethereum, SigData, Tokenizable},
	ApiCall, ChainCrypto,
};

use super::{ethabi_function, ethabi_param, EthereumReplayProtection};

use sp_runtime::RuntimeDebug;

impl Tokenizable for Vec<u8> {
	fn tokenize(self) -> Token {
		Token::Bytes(self)
	}
}

impl Tokenizable for ForeignChainAddress {
	fn tokenize(self) -> Token {
		match self {
			ForeignChainAddress::Eth(addr) =>
				Token::Tuple(vec![Token::Uint(ETHEREUM_CHAIN_ID.into()), addr.to_vec().tokenize()]),
			ForeignChainAddress::Dot(addr) =>
				Token::Tuple(vec![Token::Uint(POLKADOT_CHAIN_ID.into()), addr.to_vec().tokenize()]),
		}
	}
}

/// Represents all the arguments required to build the call to Vault's 'allBatch'
/// function.
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct ExecutexSwapAndCall {
	/// The signature data for validation and replay protection.
	sig_data: SigData,
	/// A single transfer that need to be made to given addresses.
	transfer_param: EncodableTransferAssetParams,
	/// The source of the transfer
	from: ForeignChainAddress,
	/// Message that needs to be passed through.
	message: Vec<u8>,
}

impl ExecutexSwapAndCall {
	pub(crate) fn new_unsigned(
		replay_protection: EthereumReplayProtection,
		transfer_param: EncodableTransferAssetParams,
		from: ForeignChainAddress,
		message: Vec<u8>,
	) -> Self {
		let mut calldata =
			Self { sig_data: SigData::new_empty(replay_protection), transfer_param, from, message };
		calldata.sig_data.insert_msg_hash_from(calldata.abi_encoded().as_slice());

		calldata
	}

	fn get_function(&self) -> ethabi::Function {
		ethabi_function(
			"executexSwapAndCall",
			vec![
				ethabi_param(
					"sigData",
					ParamType::Tuple(vec![
						ParamType::Address,
						ParamType::Uint(256),
						ParamType::Uint(256),
						ParamType::Uint(256),
						ParamType::Uint(256),
						ParamType::Address,
					]),
				),
				ethabi_param(
					"transferParams",
					ParamType::Tuple(vec![
						ParamType::Address,
						ParamType::Address,
						ParamType::Uint(256),
					]),
				),
				ethabi_param("srcChain", ParamType::Uint(256)),
				ethabi_param("srcAddress", ParamType::Bytes),
				ethabi_param("message", ParamType::Bytes),
			],
		)
	}

	fn abi_encoded(&self) -> Vec<u8> {
		let tokenized_address =
			self.from.clone().tokenize().into_tuple().expect(
				"The ForeignChainAddress should always return a Tuple(vec![Chain, Address])",
			);

		self.get_function()
			.encode_input(&[
				self.sig_data.tokenize(),
				self.transfer_param.clone().tokenize(),
				tokenized_address[0].clone(),
				tokenized_address[1].clone(),
				Token::Bytes(self.message.clone()),
			])
			.expect(
				r#"
						This can only fail if the parameter types don't match the function signature encoded below.
						Therefore, as long as the tests pass, it can't fail at runtime.
					"#,
			)
	}
}

impl ApiCall<Ethereum> for ExecutexSwapAndCall {
	fn threshold_signature_payload(&self) -> <Ethereum as ChainCrypto>::Payload {
		self.sig_data.msg_hash
	}

	fn signed(mut self, signature: &<Ethereum as ChainCrypto>::ThresholdSignature) -> Self {
		self.sig_data.insert_signature(signature);
		self
	}

	fn chain_encoded(&self) -> Vec<u8> {
		self.abi_encoded()
	}

	fn is_signed(&self) -> bool {
		self.sig_data.is_signed()
	}
}

#[cfg(test)]
mod test_execute_x_swap_and_execute {
	use crate::eth::SchnorrVerificationComponents;

	use super::*;
	use ethabi::Address;
	use frame_support::assert_ok;

	#[test]
	// There have been obtuse test failures due to the loading of the contract failing
	// It uses a different ethabi to the CFE, so we test separately
	fn just_load_the_contract() {
		assert_ok!(ethabi::Contract::load(
			std::include_bytes!("../../../../../engine/src/eth/abis/IVault.json").as_ref(),
		));
	}

	#[test]
	fn test_payload() {
		use crate::eth::tests::asymmetrise;
		use ethabi::Token;
		const FAKE_KEYMAN_ADDR: [u8; 20] = asymmetrise([0xcf; 20]);
		const CHAIN_ID: u64 = 1;
		const NONCE: u64 = 9;

		let dummy_transfer_asset_param = EncodableTransferAssetParams {
			asset: Address::from_slice(&[5; 20]),
			to: Address::from_slice(&[7; 20]),
			amount: 10,
		};

		let dummy_src_address = ForeignChainAddress::Dot([0xff; 32]);
		let tokenized_address = dummy_src_address
			.tokenize()
			.into_tuple()
			.expect("The ForeignChainAddress should always return a Tuple(vec![Chain, Address])");
		let dummy_message = vec![0x00, 0x01, 0x02, 0x03, 0x04];

		const FAKE_NONCE_TIMES_G_ADDR: [u8; 20] = asymmetrise([0x7f; 20]);
		const FAKE_SIG: [u8; 32] = asymmetrise([0xe1; 32]);

		let eth_vault = ethabi::Contract::load(
			std::include_bytes!("../../../../../engine/src/eth/abis/IVault.json").as_ref(),
		)
		.unwrap();

		let function_reference = eth_vault.function("executexSwapAndCall").unwrap();

		let function_runtime = ExecutexSwapAndCall::new_unsigned(
			EthereumReplayProtection {
				key_manager_address: FAKE_KEYMAN_ADDR,
				chain_id: CHAIN_ID,
				nonce: NONCE,
			},
			dummy_transfer_asset_param.clone(),
			dummy_src_address,
			dummy_message,
		);

		let expected_msg_hash = function_runtime.sig_data.msg_hash;

		assert_eq!(function_runtime.threshold_signature_payload(), expected_msg_hash);
		let runtime_payload = function_runtime
			.clone()
			.signed(&SchnorrVerificationComponents {
				s: FAKE_SIG,
				k_times_g_address: FAKE_NONCE_TIMES_G_ADDR,
			})
			.chain_encoded();

		// Ensure signing payload isn't modified by signature.
		assert_eq!(function_runtime.threshold_signature_payload(), expected_msg_hash);

		assert_eq!(
			// Our encoding:
			runtime_payload,
			// "Canonical" encoding based on the abi definition above and using the ethabi crate:
			function_reference
				.encode_input(&[
					// sigData: SigData(address, uint, uint, uint, uint, address)
					Token::Tuple(vec![
						Token::Address(FAKE_KEYMAN_ADDR.into()),
						Token::Uint(CHAIN_ID.into()),
						Token::Uint(expected_msg_hash.0.into()),
						Token::Uint(FAKE_SIG.into()),
						Token::Uint(NONCE.into()),
						Token::Address(FAKE_NONCE_TIMES_G_ADDR.into()),
					]),
					dummy_transfer_asset_param.tokenize(),
					tokenized_address[0].clone(),
					tokenized_address[1].clone(),
					dummy_message.tokenize(),
				])
				.unwrap()
		);
	}
}
