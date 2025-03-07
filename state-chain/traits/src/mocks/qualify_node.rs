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

use crate::QualifyNode;
use codec::{Decode, Encode};
use sp_std::marker::PhantomData;

use super::{MockPallet, MockPalletStorage};

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct QualifyAll<Id>(PhantomData<Id>);

impl<Id> MockPallet for QualifyAll<Id> {
	const PREFIX: &'static [u8] = b"cf-mocks//QualifyAll";
}

impl<Id: Encode + Decode> QualifyAll<Id> {
	pub fn except<I: IntoIterator<Item = Id>>(id: I) {
		<Self as MockPalletStorage>::put_storage(b"EXCEPT", b"", id.into_iter().collect::<Vec<_>>())
	}
}

impl<Id: Ord + Clone + Encode + Decode> QualifyNode<Id> for QualifyAll<Id> {
	fn is_qualified(id: &Id) -> bool {
		!Self::get_storage::<_, Vec<Id>>(b"EXCEPT", b"").unwrap_or_default().contains(id)
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn test_qualify_exclusion() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			assert!(QualifyAll::is_qualified(&1));
			assert!(QualifyAll::is_qualified(&2));
			assert!(QualifyAll::is_qualified(&3));
			QualifyAll::except([1, 2]);
			assert!(!QualifyAll::is_qualified(&1));
			assert!(!QualifyAll::is_qualified(&2));
			assert!(QualifyAll::is_qualified(&3));
		});
	}
}
