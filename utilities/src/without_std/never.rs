// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use crate::type_introspection::HasTypeIntrospection;
use sp_std::vec::Vec;

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

	fn sample_all_shapes() -> Vec<Self> {
		Vec::new()
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

pub trait IsEmptyType: Sized {
	fn as_never(&self) -> Never;
}

impl IsEmptyType for Never {
	fn as_never(&self) -> Never {
		match *self {}
	}
}
