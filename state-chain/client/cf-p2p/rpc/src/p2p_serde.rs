use serde::Serializer;

pub fn serialize_base58<S: Serializer>(bytes: impl AsRef<[u8]>, s: S) -> Result<S::Ok, S::Error> {
	s.serialize_str(&bs58::encode(bytes).into_string())
}

pub mod bs58_vec {
	pub use super::serialize_base58 as serialize;
	use serde::{Deserialize, Deserializer};

	pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
		let s = <&str>::deserialize(d)?;
		bs58::decode(s)
			.into_vec()
			.map_err(|e| serde::de::Error::custom(e))
	}
}

pub mod bs58_fixed_size {
	pub use super::serialize_base58 as serialize;
	use serde::{de::Error, Deserialize, Deserializer};

	pub fn deserialize<'de, D: Deserializer<'de>, const Size: usize>(
		d: D,
	) -> Result<[u8; Size], D::Error> {
		let mut buffer = [0xFF; Size];
		let s = <&str>::deserialize(d)?;
		let decoded = bs58::decode(s)
			.into(&mut buffer)
			.map_err(|e| Error::custom(e))?;
		if decoded != buffer.len() {
			return Err(Error::custom("not enough bytes"));
		}
		Ok(buffer)
	}
}
