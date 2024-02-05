use serde::{
	de::{Deserialize, Deserializer, Error as DeError},
	ser::{Serialize, Serializer},
};

pub fn serialize<S, const SIZE: usize>(value: &[u8; SIZE], ser: S) -> Result<S::Ok, S::Error>
where
	S: Serializer,
{
	bs58::encode(value).into_string().serialize(ser)
}

pub fn deserialize<'de, D, const SIZE: usize>(d: D) -> Result<[u8; SIZE], D::Error>
where
	D: Deserializer<'de>,
{
	let encoded = String::deserialize(d)?;

	bs58::decode(encoded)
		.into_vec()
		.map_err(D::Error::custom)?
		.try_into()
		.map_err(|invalid: Vec<u8>| D::Error::custom(format!("invalid length: {}", invalid.len())))
}
