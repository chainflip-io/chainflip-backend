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

use crate::DeregistrationCheck;
use codec::{Decode, Encode};
use sp_std::marker::PhantomData;

use super::{MockPallet, MockPalletStorage};

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct MockDeregistrationCheck<Id>(PhantomData<Id>);

impl<Id> MockPallet for MockDeregistrationCheck<Id> {
	const PREFIX: &'static [u8] = b"cf-mocks//DeregistrationCheck";
}

const SHOULD_FAIL: &[u8] = b"SHOULD_FAIL";

impl<Id: Encode + Decode> MockDeregistrationCheck<Id> {
	pub fn set_should_fail(account_id: &Id, should_fail: bool) {
		if should_fail {
			<Self as MockPalletStorage>::put_storage(SHOULD_FAIL, account_id, ());
		} else {
			Self::take_storage::<_, Id>(SHOULD_FAIL, account_id);
		}
	}
	fn should_fail(account_id: &Id) -> bool {
		<Self as MockPalletStorage>::get_storage::<_, ()>(SHOULD_FAIL, account_id).is_some()
	}
}

impl<Id: Encode + Decode> DeregistrationCheck for MockDeregistrationCheck<Id> {
	type AccountId = Id;
	type Error = &'static str;

	fn check(account_id: &Self::AccountId) -> Result<(), Self::Error> {
		if Self::should_fail(account_id) {
			Err("Cannot deregister.")
		} else {
			Ok(())
		}
	}
}
