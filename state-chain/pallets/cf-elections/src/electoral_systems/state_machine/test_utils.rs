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
use std::collections::BTreeMap;

#[derive(PartialEq, Eq, Debug)]
pub struct BTreeMultiSet<A>(pub BTreeMap<A, usize>);

impl<A> Default for BTreeMultiSet<A> {
	fn default() -> Self {
		Self(Default::default())
	}
}

impl<A: Ord> BTreeMultiSet<A> {
	pub fn insert(&mut self, a: A) {
		*self.0.entry(a).or_insert(0) += 1;
	}
}

impl<A: Ord> FromIterator<A> for BTreeMultiSet<A> {
	fn from_iter<T: IntoIterator<Item = A>>(iter: T) -> Self {
		let mut result = Self::default();
		for x in iter {
			result.insert(x);
		}
		result
	}
}
