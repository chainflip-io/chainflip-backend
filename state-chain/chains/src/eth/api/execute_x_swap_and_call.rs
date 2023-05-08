use cf_primitives::{EgressId, ForeignChain};
use codec::{Decode, Encode};
use ethabi::{Address, ParamType, Token, Uint};
use scale_info::TypeInfo;
use sp_std::{vec, vec::Vec};

use crate::{
	address::ForeignChainAddress,
	eth::{api::all_batch::EncodableTransferAssetParams, EthereumCall, Tokenizable},
};

use sp_runtime::RuntimeDebug;

impl Tokenizable for Vec<u8> {
	fn tokenize(self) -> Token {
		Token::Bytes(self)
	}

	fn param_type() -> ethabi::ParamType {
		ParamType::Bytes
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

	fn param_type() -> ethabi::ParamType {
		ParamType::Uint(32)
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

	fn param_type() -> ethabi::ParamType {
		ParamType::Tuple(vec![ForeignChain::param_type(), ParamType::Bytes])
	}
}

/// Represents all the arguments required to build the call to Vault's 'ExecutexSwapAndCall'
/// function.
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct ExecutexSwapAndCall {
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
	pub(crate) fn new(
		egress_id: EgressId,
		transfer_param: EncodableTransferAssetParams,
		source_address: ForeignChainAddress,
		message: Vec<u8>,
	) -> Self {
		Self { egress_id, transfer_param, source_address, message }
	}

	pub fn egress_id(&self) -> EgressId {
		self.egress_id
	}
}

impl EthereumCall for ExecutexSwapAndCall {
	const FUNCTION_NAME: &'static str = "executexSwapAndCall";

	fn function_params() -> Vec<(&'static str, ethabi::ParamType)> {
		vec![
			(
				"transferParams",
				ParamType::Tuple(vec![
					Address::param_type(),
					Address::param_type(),
					Uint::param_type(),
				]),
			),
			("srcChain", ParamType::Uint(32)),
			("srcAddress", ParamType::Bytes),
			("message", <Vec<u8>>::param_type()),
		]
	}

	fn function_call_args(&self) -> Vec<Token> {
		vec![
			self.transfer_param.clone().tokenize(),
			self.source_address.clone().tokenize(),
			self.message.clone().tokenize(),
		]
	}
}

#[cfg(test)]
mod test_execute_x_swap_and_execute {
	use crate::{
		eth::{
			api::EthereumReplayProtection, EthereumTransactionBuilder,
			SchnorrVerificationComponents,
		},
		ApiCall,
	};

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

		let call = ExecutexSwapAndCall::new(
			(ForeignChain::Ethereum, 0),
			dummy_transfer_asset_param.clone(),
			dummy_src_address,
			dummy_message.clone(),
		);
		let expected_msg_hash = call.msg_hash();
		let function_runtime = EthereumTransactionBuilder::new_unsigned(
			EthereumReplayProtection {
				nonce: NONCE,
				chain_id: CHAIN_ID,
				key_manager_address: FAKE_KEYMAN_ADDR.into(),
				contract_address: FAKE_VAULT_ADDR.into(),
			},
			call,
		);

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
					// sigData: SigData(uint, uint, address)
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
