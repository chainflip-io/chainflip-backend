use serde::Serializer;

pub fn serialize_base58<S: Serializer>(bytes: impl AsRef<[u8]>, s: S) -> Result<S::Ok, S::Error> {
	s.serialize_str(&bs58::encode(bytes).into_string())
}

pub mod bs58_vec {
	pub use super::serialize_base58 as serialize;
	use serde::{Deserialize, Deserializer};

	pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
		let s = String::deserialize(d)?;
		bs58::decode(s)
			.into_vec()
			.map_err(|e| serde::de::Error::custom(e))
	}
}

pub mod bs58_fixed_size {
	pub use super::serialize_base58 as serialize;
	use serde::{de::Error, Deserialize, Deserializer};

	pub fn deserialize<'de, D: Deserializer<'de>, const SIZE: usize>(
		d: D,
	) -> Result<[u8; SIZE], D::Error> {
		let mut buffer = [0xFF; SIZE];
		let s = String::deserialize(d)?;
		let decoded = bs58::decode(s)
			.into(&mut buffer)
			.map_err(|e| Error::custom(e))?;
		if decoded != buffer.len() {
			return Err(Error::custom("not enough bytes"));
		}
		Ok(buffer)
	}
}

#[test]
fn test_validator_id_bs58() {
	use super::{ValidatorIdBs58};
	use serde_json;

	let validator_id_raw = [0xCF; 32];

	serde_json::to_string(&ValidatorIdBs58(validator_id_raw))
		.expect("Encoding validator Id should work.");

	serde_json::from_str::<ValidatorIdBs58>(r#""5G""#).expect_err("Length is invalid.");
	serde_json::from_str::<ValidatorIdBs58>(r#""5G9NWJ5P9uk7am24yCKeLZJqXWW6hjuMyRJDmw4ofqx""#)
		.expect("Valid Id.");
}

#[test]
fn test_message_bs58() {
	use super::{MessageBs58};
	use serde_json;

	let original_message = b"super interesting".to_vec();

	let encoded_message = serde_json::to_string(&MessageBs58(original_message.clone())).unwrap();

	let decoded_message: MessageBs58 =
		serde_json::from_str(encoded_message.as_str()).expect("Encoded should decode.");
	assert_eq!(decoded_message.0, original_message);
}
