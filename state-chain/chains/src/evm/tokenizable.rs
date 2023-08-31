use ethabi::{ParamType, Token};
use sp_std::prelude::*;

pub trait Tokenizable {
	fn param_type() -> ethabi::ParamType;
	fn tokenize(self) -> Token;
}

impl Tokenizable for ethereum_types::U256 {
	fn tokenize(self) -> Token {
		Token::Uint(self)
	}

	fn param_type() -> ethabi::ParamType {
		ParamType::Uint(256)
	}
}

impl Tokenizable for ethereum_types::H256 {
	fn tokenize(self) -> Token {
		Token::FixedBytes(self.0.into())
	}

	fn param_type() -> ethabi::ParamType {
		ParamType::Uint(256)
	}
}

impl Tokenizable for u64 {
	fn tokenize(self) -> Token {
		Token::Uint(self.into())
	}

	fn param_type() -> ethabi::ParamType {
		ParamType::Uint(256)
	}
}

impl Tokenizable for ethereum_types::Address {
	fn tokenize(self) -> Token {
		Token::Address(self)
	}

	fn param_type() -> ethabi::ParamType {
		ParamType::Address
	}
}

impl Tokenizable for ethabi::Function {
	fn tokenize(self) -> Token {
		Token::FixedBytes(self.short_signature().to_vec())
	}

	fn param_type() -> ethabi::ParamType {
		ParamType::FixedBytes(4)
	}
}

impl<const S: usize> Tokenizable for [u8; S] {
	fn param_type() -> ethabi::ParamType {
		ParamType::FixedBytes(S)
	}

	fn tokenize(self) -> Token {
		Token::FixedBytes(self.to_vec())
	}
}

impl Tokenizable for Vec<u8> {
	fn tokenize(self) -> Token {
		Token::Bytes(self)
	}

	fn param_type() -> ethabi::ParamType {
		ParamType::Bytes
	}
}

impl Tokenizable for u32 {
	fn tokenize(self) -> Token {
		Token::Uint(self.into())
	}

	fn param_type() -> ethabi::ParamType {
		ParamType::Uint(32)
	}
}

impl<T: Tokenizable> Tokenizable for Vec<T> {
	fn tokenize(self) -> Token {
		Token::Array(self.into_iter().map(|t| t.tokenize()).collect())
	}

	fn param_type() -> ethabi::ParamType {
		ParamType::Array(Box::new(T::param_type()))
	}
}
