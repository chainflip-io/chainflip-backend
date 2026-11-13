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

use crate::DelegationInfo;
use codec::{Decode, Encode};
use sp_std::marker::PhantomData;

use super::{MockPallet, MockPalletStorage};

pub struct MockDelegationInfo<Id>(PhantomData<Id>);

impl<Id> MockPallet for MockDelegationInfo<Id> {
	const PREFIX: &'static [u8] = b"cf-mocks//DelegationInfo";
}

const DELEGATING: &[u8] = b"DELEGATING";

impl<Id: Encode + Decode> MockDelegationInfo<Id> {
	pub fn set_delegating(account_id: &Id, delegating: bool) {
		if delegating {
			Self::put_storage(DELEGATING, account_id, ());
		} else {
			Self::take_storage::<_, ()>(DELEGATING, account_id);
		}
	}
}

impl<Id: Encode + Decode> DelegationInfo for MockDelegationInfo<Id> {
	type AccountId = Id;

	fn is_delegating(account_id: &Self::AccountId) -> bool {
		Self::get_storage::<_, ()>(DELEGATING, account_id).is_some()
	}
}
