use cf_primitives::{EgressId, ForeignChain};
use codec::{Decode, Encode};
use ethabi::{encode, Address, ParamType, Token};
use scale_info::TypeInfo;
use sp_std::{vec, vec::Vec};

use crate::{
	address::ForeignChainAddress,
	eth::{
		api::all_batch::EncodableTransferAssetParams, Ethereum, EthereumSignatureHandler,
		Tokenizable,
	},
	impl_api_call_eth, ApiCall, ChainCrypto,
};

use super::{ethabi_function, ethabi_param, EthereumReplayProtection};

use sp_runtime::RuntimeDebug;

impl Tokenizable for Vec<u8> {
	fn tokenize(self) -> Token {
		Token::Bytes(self)
	}
}

impl Tokenizable for ForeignChain {
	fn tokenize(self) -> Token {
		match self {
			// TODO: Confirm integer representaiton of foreign chains.
			ForeignChain::Ethereum => Token::Uint(1.into()),
			ForeignChain::Polkadot => Token::Uint(2.into()),
			ForeignChain::Bitcoin => Token::Uint(3.into()),
		}
	}
}

impl Tokenizable for ForeignChainAddress {
	fn tokenize(self) -> Token {
		match self {
			ForeignChainAddress::Eth(addr) =>
				Token::Tuple(vec![ForeignChain::Ethereum.tokenize(), addr.to_vec().tokenize()]),
			ForeignChainAddress::Dot(addr) =>
				Token::Tuple(vec![ForeignChain::Polkadot.tokenize(), addr.to_vec().tokenize()]),
			ForeignChainAddress::Btc(addr) =>
				Token::Tuple(vec![ForeignChain::Bitcoin.tokenize(), addr.encode().tokenize()]),
		}
	}
}

/// Represents all the arguments required to build the call to Vault's 'ExecutexSwapAndCall'
/// function.
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct ExecutexSwapAndCall {
	/// The signature handler for creating payload and inserting signature.
	pub signature_handler: EthereumSignatureHandler,
	/// The egress Id. Used to query Gas budge stored in the Ccm Pallet.
	egress_id: EgressId,
	/// A single transfer that need to be made to given addresses.
	transfer_param: EncodableTransferAssetParams,
	/// The source of the transfer
	source_address: ForeignChainAddress,
	/// Message that needs to be passed through.
	message: Vec<u8>,
}

impl ExecutexSwapAndCall {
	#[allow(clippy::too_many_arguments)]
	pub(crate) fn new_unsigned(
		replay_protection: EthereumReplayProtection,
		egress_id: EgressId,
		transfer_param: EncodableTransferAssetParams,
		source_address: ForeignChainAddress,
		message: Vec<u8>,
		key_manager_address: Address,
		vault_contract_address: Address,
		ethereum_chain_id: u64,
	) -> Self {
		Self {
			signature_handler: EthereumSignatureHandler::new_unsigned(
				replay_protection,
				Self::abi_encoded_for_payload(
					transfer_param.clone(),
					source_address.clone(),
					message.clone(),
				),
				key_manager_address,
				vault_contract_address,
				ethereum_chain_id,
			),
			egress_id,
			transfer_param,
			source_address,
			message,
		}
	}

	pub fn egress_id(&self) -> EgressId {
		self.egress_id
	}

	fn get_function() -> ethabi::Function {
		ethabi_function(
			"executexSwapAndCall",
			vec![
				ethabi_param(
					"sigData",
					ParamType::Tuple(vec![
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
				ethabi_param("srcChain", ParamType::Uint(32)),
				ethabi_param("srcAddress", ParamType::Bytes),
				ethabi_param("message", ParamType::Bytes),
			],
		)
	}

	fn abi_encoded(&self) -> Vec<u8> {
		let tokenized_address =
			self.source_address.clone().tokenize().into_tuple().expect(
				"The ForeignChainAddress should always return a Tuple(vec![Chain, Address])",
			);

		Self::get_function()
			.encode_input(&[
				self.signature_handler.sig_data.tokenize(),
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

	fn abi_encoded_for_payload(
		transfer_param: EncodableTransferAssetParams,
		source_address: ForeignChainAddress,
		message: Vec<u8>,
	) -> Vec<u8> {
		let tokenized_address = source_address
			.tokenize()
			.into_tuple()
			.expect("The ForeignChainAddress should always return a Tuple(vec![Chain, Address])");
		Self::get_function()
			.short_signature()
			.into_iter()
			.chain(encode(&[
				transfer_param.tokenize(),
				tokenized_address[0].clone(),
				tokenized_address[1].clone(),
				Token::Bytes(message),
			]))
			.collect()
	}
}

impl_api_call_eth!(ExecutexSwapAndCall);

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
			std::include_bytes!("../../../../../engine/src/eth/abis/Vault.json").as_ref(),
		));
	}

	#[test]
	fn test_payload() {
		use crate::eth::tests::asymmetrise;
		use ethabi::Token;
		const FAKE_KEYMAN_ADDR: [u8; 20] = asymmetrise([0xcf; 20]);
		const FAKE_VAULT_ADDR: [u8; 20] = asymmetrise([0xdf; 20]);
		const CHAIN_ID: u64 = 1;
		const NONCE: u64 = 9;

		let dummy_transfer_asset_param = EncodableTransferAssetParams {
			asset: Address::from_slice(&[5; 20]),
			to: Address::from_slice(&[7; 20]),
			amount: 10,
		};

		let dummy_src_address = ForeignChainAddress::Dot([0xff; 32]);
		let tokenized_address =
			dummy_src_address.clone().tokenize().into_tuple().expect(
				"The ForeignChainAddress should always return a Tuple(vec![Chain, Address])",
			);
		let dummy_message = vec![0x00, 0x01, 0x02, 0x03, 0x04];

		const FAKE_NONCE_TIMES_G_ADDR: [u8; 20] = asymmetrise([0x7f; 20]);
		const FAKE_SIG: [u8; 32] = asymmetrise([0xe1; 32]);

		let eth_vault = ethabi::Contract::load(
			std::include_bytes!("../../../../../engine/src/eth/abis/Vault.json").as_ref(),
		)
		.unwrap();

		let function_reference = eth_vault.function("executexSwapAndCall").unwrap();

		let function_runtime = ExecutexSwapAndCall::new_unsigned(
			EthereumReplayProtection { nonce: NONCE },
			(ForeignChain::Ethereum, 0),
			dummy_transfer_asset_param.clone(),
			dummy_src_address,
			dummy_message.clone(),
			FAKE_KEYMAN_ADDR.into(),
			FAKE_VAULT_ADDR.into(),
			CHAIN_ID,
		);

		let expected_msg_hash = function_runtime.signature_handler.payload;

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
