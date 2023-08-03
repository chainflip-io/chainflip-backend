///! Serialization and deserialization of numbers as `NumberOrHex`.
///!
///! Json numbers are limited to 64 bits. This module can be used in serde annotations to
///! serialize and deserialize large numbers as hex instead.
///!
///! ```example
///! use serde::{Deserialize, Serialize};
///!
///! #[derive(Debug, Clone, Serialize, Deserialize)]
///! struct Foo {
///!     #[serde(with = "cf_utilities::number_or_hex")]
///!     bar: u128,
///! }
/// ```
use serde::{de::Error, Deserialize, Deserializer, Serialize, Serializer};
use sp_rpc::number::NumberOrHex;

pub fn serialize<S, T>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
where
	S: Serializer,
	NumberOrHex: From<T>,
	T: Copy,
{
	NumberOrHex::from(*value).serialize(serializer)
}

pub fn deserialize<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
	D: Deserializer<'de>,
	T: TryFrom<NumberOrHex>,
{
	let value = NumberOrHex::deserialize(deserializer)?;
	T::try_from(value).map_err(|_| D::Error::custom("Failed to deserialize number."))
}
