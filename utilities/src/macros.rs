/// Adds #[derive] statements for commonly used traits. These are currently: Debug, Clone,
/// PartialEq, Eq, Encode, Decode, Serialize, Deserialize
#[macro_export]
macro_rules! derive_common_traits {
	($($Definition:tt)*) => {
		#[derive(
			Debug, Clone, PartialEq, Eq, Encode, Decode,
		)]
		#[derive(Deserialize, Serialize)]
		#[serde(bound(deserialize = "", serialize = ""))]
		$($Definition)*
	};
}
pub use derive_common_traits;
