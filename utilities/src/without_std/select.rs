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
use sp_std::{boxed::Box, collections::btree_map::BTreeMap};

/// Can be used to control which values are extracted from a key-value map.
///
/// Useful when e.g. processing rpc methods, where passing a parameter is
/// a request for this particular key but passing nothing is meant to return all
/// all the information for all keys.
pub enum Select<'k, Key: 'k> {
	Single(&'k Key),
	All(),
}

impl<'k, Key: Ord + 'k> Select<'k, Key> {
	pub fn select_values_from_btree_map<'a: 'k, A: 'a>(
		&self,
		container: &'a BTreeMap<Key, A>,
	) -> Box<dyn Iterator<Item = (&'a Key, &'a A)> + 'a> {
		match self {
			Select::Single(key) => Box::new(container.get_key_value(key).into_iter()),
			Select::All() => Box::new(container.iter()),
		}
	}
}
