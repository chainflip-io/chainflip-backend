use super::*;
use crate::cf_parameters::VersionedCfParameters;
use cf_primitives::ForeignChain;
use codec::{Decode, Encode};
use ethabi::Token;
use frame_support::sp_runtime::RuntimeDebug;
use scale_info::TypeInfo;
use sp_std::{vec, vec::Vec};

/// Represents all the arguments required to build the call to Vault's 'xSwapToken'
/// function.
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct XSwapToken {
	destination_chain: u32,
	destination_address: Vec<u8>,
	destination_token: u32,
	source_token: Vec<u8>,
	amount: U256,
	cf_parameters: Vec<u8>,
}

impl XSwapToken {
	pub fn new(
		destination_chain: ForeignChain,
		destination_address: EncodedAddress,
		destination_token: Asset,
		source_token: ethereum_types::Address,
		amount: <Ethereum as Chain>::ChainAmount,
		cf_parameters: VersionedCfParameters,
	) -> Self {
		Self {
			destination_chain: destination_chain as u32,
			destination_address: destination_address.into_vec(),
			destination_token: destination_token as u32,
			source_token: source_token.as_bytes().to_vec(),
			amount: U256::from(amount),
			cf_parameters: cf_parameters.encode(),
		}
	}
}

impl EvmCall for XSwapToken {
	const FUNCTION_NAME: &'static str = "xSwapToken";

	fn function_params() -> Vec<(&'static str, ethabi::ParamType)> {
		vec![
			("dstChain", u32::param_type()),
			("dstAddress", <Vec<u8>>::param_type()),
			("dstToken", u32::param_type()),
			("srcToken", <Vec<u8>>::param_type()),
			("amount", U256::param_type()),
			("cfParameters", <Vec<u8>>::param_type()),
		]
	}

	fn function_call_args(&self) -> Vec<Token> {
		vec![
			self.destination_chain.tokenize(),
			self.destination_address.clone().tokenize(),
			self.destination_token.tokenize(),
			self.source_token.clone().tokenize(),
			self.amount.tokenize(),
			self.cf_parameters.clone().tokenize(),
		]
	}
}

// TODO JAMIE: tests?
