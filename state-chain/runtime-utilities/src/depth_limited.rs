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

use codec::{Decode, DecodeLimit, DecodeWithMemTracking, Input, WrapperTypeEncode};
use scale_info::{Type, TypeInfo};
use sp_std::ops::Deref;

/// Decodes `T` with codec's depth tracking limited to `LIMIT`, so a recursive type from untrusted
/// input, such as an extrinsic parameter, can't nest up to the default extrinsic depth limit (256).
/// Encodes, and appears in metadata, as the inner `T`.
///
/// Codec counts a level for each collection of non-primitive items, even an empty one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DepthLimited<T, const LIMIT: u32>(pub T);

impl<T, const LIMIT: u32> From<T> for DepthLimited<T, LIMIT> {
	fn from(value: T) -> Self {
		Self(value)
	}
}

impl<T, const LIMIT: u32> Deref for DepthLimited<T, LIMIT> {
	type Target = T;

	fn deref(&self) -> &T {
		&self.0
	}
}

impl<T, const LIMIT: u32> WrapperTypeEncode for DepthLimited<T, LIMIT> {}

impl<T: Decode, const LIMIT: u32> Decode for DepthLimited<T, LIMIT> {
	fn decode<I: Input>(input: &mut I) -> Result<Self, codec::Error> {
		T::decode_with_depth_limit(LIMIT, input).map(Self)
	}
}

/// The depth tracker forwards allocation tracking to the input it wraps.
impl<T: DecodeWithMemTracking, const LIMIT: u32> DecodeWithMemTracking for DepthLimited<T, LIMIT> {}

impl<T: TypeInfo + 'static, const LIMIT: u32> TypeInfo for DepthLimited<T, LIMIT> {
	type Identity = T;

	fn type_info() -> Type {
		T::type_info()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::Encode;
	use sp_std::vec;

	// Two levels: the inner `Vec<u32>` holds primitives, which codec doesn't count.
	type Nested = Vec<Vec<Vec<u32>>>;

	#[test]
	fn is_transparent() {
		let value: Nested = vec![vec![vec![1, 2]], vec![]];
		assert_eq!(DepthLimited::<_, 2>(value.clone()).encode(), value.encode());
		assert_eq!(DepthLimited::<Nested, 2>::type_info(), Nested::type_info());
	}

	#[test]
	fn rejects_input_nested_beyond_the_limit() {
		let encoded = Nested::from([vec![vec![1, 2]], vec![]]).encode();
		assert_eq!(
			DepthLimited::<Nested, 2>::decode(&mut &encoded[..]),
			Ok(DepthLimited(vec![vec![vec![1, 2]], vec![]]))
		);
		assert!(DepthLimited::<Nested, 1>::decode(&mut &encoded[..]).is_err());
	}
}
