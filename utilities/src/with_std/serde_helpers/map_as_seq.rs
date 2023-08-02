use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;

pub fn serialize<K, V, S>(map: &BTreeMap<K, V>, serializer: S) -> Result<S::Ok, S::Error>
where
	K: Serialize,
	V: Serialize,
	S: Serializer,
{
	serializer.collect_seq(map)
}

pub fn deserialize<'de, K, V, D>(deserializer: D) -> Result<BTreeMap<K, V>, D::Error>
where
	K: DeserializeOwned + Ord,
	V: DeserializeOwned,
	D: Deserializer<'de>,
{
	BTreeMap::deserialize(deserializer)
}
