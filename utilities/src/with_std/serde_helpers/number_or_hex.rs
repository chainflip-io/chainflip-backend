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

//! Serialization and deserialization of numbers as `NumberOrHex`.
//!
//! Json numbers are limited to 64 bits. This module can be used in serde annotations to
//! serialize and deserialize large numbers as hex instead.
//!
//! ```example
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Debug, Clone, Serialize, Deserialize)]
//! struct Foo {
//!     #[serde(with = "cf_utilities::number_or_hex")]
//!     bar: u128,
//! }
/// ```
use crate::rpc::NumberOrHex;
use serde::{de::Error, Deserialize, Deserializer, Serialize, Serializer};

pub fn serialize<S, T>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
where
	S: Serializer,
	NumberOrHex: From<T>,
	T: Copy,
{
	NumberOrHex::from(*value).serialize(serializer)
}

pub fn deserialize<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
	D: Deserializer<'de>,
	T: TryFrom<NumberOrHex>,
{
	let value = NumberOrHex::deserialize(deserializer)?;
	T::try_from(value).map_err(|_| D::Error::custom("Failed to deserialize number."))
}
