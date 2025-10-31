//! Some convenient serde helpers

use ethabi::ethereum_types::U256;
use scale_info::prelude::string::{String, ToString};
use serde::{Deserialize, Deserializer};
use sp_std::str::FromStr;

/// Helper type to parse both `u64` and `U256`
#[derive(Debug, Copy, Clone, Deserialize)]
#[serde(untagged)]
pub enum Numeric {
	U256(U256),
	Num(u64),
}

impl From<Numeric> for U256 {
	fn from(n: Numeric) -> U256 {
		match n {
			Numeric::U256(n) => n,
			Numeric::Num(n) => U256::from(n),
		}
	}
}

impl FromStr for Numeric {
	type Err = String;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		if let Ok(val) = s.parse::<u128>() {
			Ok(Numeric::U256(val.into()))
		} else if s.starts_with("0x") {
			U256::from_str(s).map(Numeric::U256).map_err(|err| err.to_string())
		} else {
			U256::from_dec_str(s).map(Numeric::U256).map_err(|err| err.to_string())
		}
	}
}

/// Helper type to parse numeric strings, `u64` and `U256`
#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum StringifiedNumeric {
	String(String),
	U256(Numeric),
	Num(serde_json::Number),
}

impl TryFrom<StringifiedNumeric> for U256 {
	type Error = String;

	fn try_from(value: StringifiedNumeric) -> Result<Self, Self::Error> {
		match value {
			StringifiedNumeric::U256(n) => Ok(n.into()),
			StringifiedNumeric::Num(n) =>
				Ok(U256::from_dec_str(&n.to_string()).map_err(|err| err.to_string())?),
			StringifiedNumeric::String(s) =>
				if let Ok(val) = s.parse::<u128>() {
					Ok(val.into())
				} else if s.starts_with("0x") {
					U256::from_str(&s).map_err(|err| err.to_string())
				} else {
					U256::from_dec_str(&s).map_err(|err| err.to_string())
				},
		}
	}
}

/// Supports parsing numbers as strings
///
/// See <https://github.com/gakonst/ethers-rs/issues/1507>
pub fn deserialize_stringified_numeric_opt<'de, D>(
	deserializer: D,
) -> Result<Option<U256>, D::Error>
where
	D: Deserializer<'de>,
{
	if let Some(num) = Option::<StringifiedNumeric>::deserialize(deserializer)? {
		num.try_into().map(Some).map_err(serde::de::Error::custom)
	} else {
		Ok(None)
	}
}
