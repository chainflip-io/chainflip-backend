use super::*;
use crate::eth::deposit_address::get_salt;
use cf_primitives::{AssetAmount, ChannelId};
use ethabi::{Address, ParamType, Token, Uint};

#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, Default, PartialEq, Eq)]
pub(crate) struct EncodableFetchAssetParams {
	pub contract_address: Address,
	pub asset: Address,
}

impl Tokenizable for EncodableFetchAssetParams {
	fn tokenize(self) -> Token {
		Token::Tuple(vec![Token::Address(self.contract_address), Token::Address(self.asset)])
	}

	fn param_type() -> ethabi::ParamType {
		ParamType::Tuple(vec![ParamType::Address, ParamType::Address])
	}
}

#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, Default, PartialEq, Eq)]
pub(crate) struct EncodableFetchDeployAssetParams {
	pub channel_id: ChannelId,
	pub asset: Address,
}

impl Tokenizable for EncodableFetchDeployAssetParams {
	fn tokenize(self) -> Token {
		Token::Tuple(vec![
			Token::FixedBytes(get_salt(self.channel_id).to_vec()),
			Token::Address(self.asset),
		])
	}

	fn param_type() -> ethabi::ParamType {
		ParamType::Tuple(vec![ParamType::FixedBytes(32), ParamType::Address])
	}
}

#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, Default, PartialEq, Eq)]
pub(crate) struct EncodableTransferAssetParams {
	/// For EVM, the asset is encoded as a contract address.
	pub asset: Address,
	pub to: Address,
	pub amount: AssetAmount,
}

impl Tokenizable for EncodableTransferAssetParams {
	fn tokenize(self) -> Token {
		Token::Tuple(vec![
			Token::Address(self.asset),
			Token::Address(self.to),
			Token::Uint(Uint::from(self.amount)),
		])
	}

	fn param_type() -> ethabi::ParamType {
		ParamType::Tuple(vec![ParamType::Address, ParamType::Address, ParamType::Uint(256)])
	}
}
