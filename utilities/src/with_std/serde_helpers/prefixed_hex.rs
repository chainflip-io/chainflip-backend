pub fn serialize<S, T>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
where
	S: serde::Serializer,
	T: AsRef<[u8]>,
{
	serializer.serialize_str(&format!("0x{}", hex::encode(value.as_ref())))
}

pub fn deserialize<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
	D: serde::Deserializer<'de>,
	T: TryFrom<Vec<u8>>,
{
	let s: &str = serde::Deserialize::deserialize(deserializer)?;
	let s = s.strip_prefix("0x").unwrap_or(s);
	T::try_from(hex::decode(s).map_err(serde::de::Error::custom)?)
		.map_err(|_| serde::de::Error::custom("Unexpected address length"))
}

#[cfg(test)]
mod test {
	use serde::{Deserialize, Serialize};

	#[derive(Serialize, Deserialize, PartialEq, Debug)]
	struct Test(#[serde(with = "super")] [u8; 2]);

	#[test]
	fn test() {
		let val = Test([0xcf; 2]);
		let serialized = serde_json::to_string(&val).unwrap();
		assert_eq!(serialized, "\"0xcfcf\"");

		let deserialized: Test = serde_json::from_str(&serialized).unwrap();
		assert_eq!(deserialized, val);
	}
}
