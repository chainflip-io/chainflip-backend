use codec::{Decode, Encode};
use ethabi::ParamType;
use scale_info::TypeInfo;
use sp_std::{boxed::Box, vec, vec::Vec};

use crate::{
	eth::{SigData, Tokenizable},
	ApiCall, ChainAbi, ChainCrypto, Ethereum,
};

use crate::{FetchAssetParams, TransferAssetParams};

use super::{ethabi_function, ethabi_param, EthereumReplayProtection};

use sp_runtime::RuntimeDebug;

/// Represents all the arguments required to build the call to Vault's 'allBatch'
/// function.
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, Default, PartialEq, Eq)]
pub struct AllBatch {
	/// The signature data for validation and replay protection.
	pub sig_data: SigData,
	/// The list of all inbound deposits that are to be fetched in this batch call.
	pub fetch_params: Vec<FetchAssetParams<Ethereum>>,
	/// The list of all outbound transfers that need to be made to given addresses.
	pub transfer_params: Vec<TransferAssetParams<Ethereum>>,
}

impl AllBatch {
	pub fn new_unsigned(
		replay_protection: EthereumReplayProtection,
		fetch_params: Vec<FetchAssetParams<Ethereum>>,
		transfer_params: Vec<TransferAssetParams<Ethereum>>,
	) -> Self {
		let mut calldata =
			Self { sig_data: SigData::new_empty(replay_protection), fetch_params, transfer_params };
		calldata.sig_data.insert_msg_hash_from(calldata.abi_encoded().as_slice());

		calldata
	}

	fn get_function(&self) -> ethabi::Function {
		ethabi_function(
			"allBatch",
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
					"fetchParamsArray",
					ParamType::Array(Box::new(ParamType::Tuple(vec![
						ParamType::FixedBytes(32),
						ParamType::Address,
					]))),
				),
				ethabi_param(
					"transferParamsArray",
					ParamType::Array(Box::new(ParamType::Tuple(vec![
						ParamType::Address,
						ParamType::Address,
						ParamType::Uint(256),
					]))),
				),
			],
		)
	}
}

impl ApiCall<Ethereum> for AllBatch {
	fn threshold_signature_payload(&self) -> <Ethereum as ChainCrypto>::Payload {
		self.sig_data.msg_hash
	}

	fn signed(mut self, signature: &<Ethereum as ChainCrypto>::ThresholdSignature) -> Self {
		self.sig_data.insert_signature(signature);
		self
	}

	fn abi_encoded(&self) -> <Ethereum as ChainAbi>::SignedTransaction {
		self.get_function()
			.encode_input(&[
				self.sig_data.tokenize(),
				self.fetch_params.clone().tokenize(),
				self.transfer_params.clone().tokenize(),
			])
			.expect(
				r#"
						This can only fail if the parameter types don't match the function signature encoded below.
						Therefore, as long as the tests pass, it can't fail at runtime.
					"#,
			)
	}

	fn is_signed(&self) -> bool {
		self.sig_data.is_signed()
	}
}
