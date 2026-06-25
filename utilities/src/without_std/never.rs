use crate::type_introspection::HasTypeIntrospection;

/// Uninhabited type used as a placeholder for enum variants that cannot be constructed.
///
/// Unlike `!`, this implements `Encode`, `Decode`, `DecodeWithMemTracking`, `HasTypeIntrospection`,
/// `Arbitrary`, and all standard derives, so it satisfies all bounds required by the migration
/// system's generic `Enum` type.
#[derive(
	Copy,
	Clone,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Hash,
	Debug,
	codec::Encode,
	codec::Decode,
	codec::DecodeWithMemTracking,
	codec::MaxEncodedLen,
	scale_info::TypeInfo,
	serde::Serialize,
	serde::Deserialize,
)]
pub enum Never {}

impl HasTypeIntrospection for Never {
	fn is_empty_type() -> bool {
		true
	}
}

#[cfg(any(test, all(feature = "proptest", feature = "std")))]
impl proptest::arbitrary::Arbitrary for Never {
	type Parameters = ();
	type Strategy = proptest::strategy::BoxedStrategy<Self>;

	fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
		panic!("Cannot generate arbitrary values for uninhabited type Never")
	}
}
