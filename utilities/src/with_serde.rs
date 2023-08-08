use core::fmt;
use std::path::PathBuf;

// We use PathBuf because the value must be Sized, Path is not Sized
pub fn deser_path<'de, D>(deserializer: D) -> std::result::Result<PathBuf, D::Error>
where
	D: serde::Deserializer<'de>,
{
	struct PathVisitor;

	impl<'de> serde::de::Visitor<'de> for PathVisitor {
		type Value = PathBuf;

		fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
			formatter.write_str("A string containing a path")
		}

		fn visit_str<E>(self, v: &str) -> std::result::Result<Self::Value, E>
		where
			E: serde::de::Error,
		{
			Ok(PathBuf::from(v))
		}
	}

	// use our visitor to deserialize a `PathBuf`
	deserializer.deserialize_any(PathVisitor)
}
