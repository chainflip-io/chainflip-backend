use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use sp_core::U256;
use sp_rpc::number::NumberOrHex;

pub struct SafeNumberOrHex(NumberOrHex);

impl Serialize for SafeNumberOrHex {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		match self.0 {
			NumberOrHex::Number(n) if n >= 2u64.pow(53) => U256::from(n).serialize(serializer),
			NumberOrHex::Number(n) => n.serialize(serializer),
			NumberOrHex::Hex(n) => n.serialize(serializer),
		}
	}
}

impl<'de> Deserialize<'de> for SafeNumberOrHex {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		Ok(SafeNumberOrHex(NumberOrHex::deserialize(deserializer)?))
	}
}

macro_rules! impl_safe_number_or_hex {
	( $( $int:ident ),+ ) => {
		$(
			impl From<$int> for SafeNumberOrHex {
				fn from(value: $int) -> Self {
          SafeNumberOrHex(value.into())
				}
			}
		)+
  }
}

impl_safe_number_or_hex!(u32, u64, u128);

impl TryInto<u128> for SafeNumberOrHex {
	type Error = anyhow::Error;

	fn try_into(self) -> Result<u128, Self::Error> {
		u128::try_from(self.0).map_err(|_| {
			anyhow!("Error parsing amount. Please use a valid number or hex string as input.")
		})
	}
}
