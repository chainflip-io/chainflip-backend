use serde::{Deserialize, Serialize};
use sp_core::U256;

#[derive(Debug, PartialEq, Eq, Deserialize, Copy, Clone)]
#[serde(untagged)]
pub enum NumberOrHex {
	Number(u64),
	Hex(U256),
}

impl Serialize for NumberOrHex {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		match self {
			// JS numbers are 64-bit floats, so we need to use a string for numbers larger than 2^53
			&Self::Number(n) if n >= 2u64.pow(53) => U256::from(n).serialize(serializer),
			Self::Number(n) => n.serialize(serializer),
			Self::Hex(n) => n.serialize(serializer),
		}
	}
}

macro_rules! impl_safe_number {
	( $( $int:ident ),+ ) => {
		$(
			impl From<$int> for NumberOrHex {
				fn from(value: $int) -> Self {
          Self::Number(value.into())
				}
			}
		)+
  }
}

impl_safe_number!(u32, u64);

macro_rules! impl_safe_hex {
	( $( $int:ident ),+ ) => {
		$(
			impl From<$int> for NumberOrHex {
				fn from(value: $int) -> Self {
          Self::Hex(value.into())
				}
			}
		)+
  }
}

impl_safe_hex!(u128, U256);

impl TryFrom<NumberOrHex> for u128 {
	type Error = &'static str;

	fn try_from(value: NumberOrHex) -> Result<Self, Self::Error> {
		match value {
			NumberOrHex::Number(n) => Ok(n.into()),
			NumberOrHex::Hex(n) => u128::try_from(n),
		}
	}
}

impl TryFrom<NumberOrHex> for u64 {
	type Error = &'static str;

	fn try_from(value: NumberOrHex) -> Result<Self, Self::Error> {
		match value {
			NumberOrHex::Number(n) => Ok(n),
			NumberOrHex::Hex(n) => u64::try_from(n),
		}
	}
}

#[cfg(test)]
mod test {
	use super::*;

	fn assert_deser(string: &str, value: NumberOrHex) {
		assert_eq!(serde_json::to_string(&value).unwrap(), string);
		assert_eq!(serde_json::from_str::<NumberOrHex>(string).unwrap(), value);
	}

	#[test]
	fn test_serialization() {
		assert_deser("\"0x20000000000000\"", NumberOrHex::Hex(2u64.pow(53).into()));
		assert_deser("9007199254740991", NumberOrHex::Number(2u64.pow(53) - 1));
		assert_deser(
			"\"0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\"",
			NumberOrHex::Hex(U256::MAX),
		);
		assert_deser(r#""0x1234""#, NumberOrHex::Hex(0x1234.into()));
		assert_deser(r#""0x0""#, NumberOrHex::Hex(0.into()));
		assert_deser(r#"5"#, NumberOrHex::Number(5));
		assert_deser(r#"10000"#, NumberOrHex::Number(10000));
		assert_deser(r#"0"#, NumberOrHex::Number(0));
		assert_deser(r#"1000000000000"#, NumberOrHex::Number(1000000000000));
	}

	#[test]
	fn test_conversions() {
		assert_eq!(u128::try_from(NumberOrHex::Hex(u128::MAX.into())).unwrap(), u128::MAX);
		assert!(u128::try_from(NumberOrHex::Hex(U256::MAX / 2)).is_err());

		assert_eq!(u64::try_from(NumberOrHex::Hex(u64::MAX.into())).unwrap(), u64::MAX);
		assert_eq!(u64::try_from(NumberOrHex::Number(u64::MAX)).unwrap(), u64::MAX);
		assert!(u64::try_from(NumberOrHex::Hex((u128::MAX / 2).into())).is_err());

		assert_eq!(u128::try_from(NumberOrHex::Number(0)).unwrap(), 0);
		assert_eq!(u128::try_from(NumberOrHex::Hex(U256::zero())).unwrap(), 0);
	}
}
