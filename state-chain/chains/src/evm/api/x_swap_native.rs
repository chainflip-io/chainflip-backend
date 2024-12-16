use super::*;
use crate::cf_parameters::VersionedCfParameters;
use cf_primitives::ForeignChain;
use codec::{Decode, Encode};
use ethabi::Token;
use frame_support::sp_runtime::RuntimeDebug;
use scale_info::TypeInfo;
use sp_std::{vec, vec::Vec};

/// Represents all the arguments required to build the call to Vault's 'xSwapNative'
/// function.
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct XSwapNative {
	destination_chain: u32,
	destination_address: Vec<u8>,
	destination_token: u32,
	cf_parameters: Vec<u8>,
}

impl XSwapNative {
	pub fn new(
		destination_chain: ForeignChain,
		destination_address: EncodedAddress,
		destination_token: Asset,
		cf_parameters: VersionedCfParameters,
	) -> Self {
		Self {
			destination_chain: destination_chain as u32,
			destination_address: destination_address.into_vec(),
			destination_token: destination_token as u32,
			cf_parameters: cf_parameters.encode(),
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
			self.destination_chain.clone().tokenize(),
			self.destination_address.clone().tokenize(),
			self.destination_token.clone().tokenize(),
			self.cf_parameters.clone().tokenize(),
		]
	}
}

// TODO JAMIE: tests?
