use crate::{eth::{Tokenizable, self}, ApiCall, ChainAbi, ChainCrypto, Ethereum};
use codec::{Decode, Encode, MaxEncodedLen};
use ethabi::{ParamType, Token};
use frame_support::RuntimeDebug;
use scale_info::TypeInfo;
use sp_std::vec;

use crate::eth::SigData;

use super::{ethabi_function, ethabi_param, EthereumReplayProtection};

#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct SetCommunityKey {
	/// The signature data for validation and replay protection.
	pub sig_data: SigData,
	/// The new public key.
	pub new_key: eth::Address,
}

impl SetCommunityKey {
    pub fn new_unsigned(
		replay_protection: EthereumReplayProtection,
		new_key: eth::Address,
	) -> Self {
		let mut calldata =
			Self { sig_data: SigData::new_empty(replay_protection), new_key: new_key.into() };
		calldata.sig_data.insert_msg_hash_from(calldata.abi_encoded().as_slice());
		calldata
	}
    fn get_function(&self) -> ethabi::Function {
		ethabi_function(
			"SetCommunityKey",
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
					"newKey",
					ParamType::Tuple(vec![ParamType::Uint(256), ParamType::Uint(8)]),
				),
			],
		)
	}
}

impl ApiCall<Ethereum> for SetCommunityKey {
	fn threshold_signature_payload(&self) -> <Ethereum as ChainCrypto>::Payload {
		self.sig_data.msg_hash
	}

	fn signed(mut self, signature: &<Ethereum as ChainCrypto>::ThresholdSignature) -> Self {
		self.sig_data.insert_signature(signature);
		self
	}

	fn abi_encoded(&self) -> <Ethereum as ChainAbi>::SignedTransaction {
		self.get_function()
			.encode_input(&[self.sig_data.tokenize(), Token::Address(self.new_key)])
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