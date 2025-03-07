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

use core::marker::PhantomData;

use crate::{Chainflip, ReputationResetter};

use super::{MockPallet, MockPalletStorage};

pub struct MockReputationResetter<T: Chainflip>(PhantomData<T>);

impl<T: Chainflip> MockPallet for MockReputationResetter<T> {
	const PREFIX: &'static [u8] = b"MockReputationResetter";
}

const REPUTATION: &[u8] = b"Reputation";

impl<T: Chainflip> MockReputationResetter<T> {
	pub fn reputation_was_reset() -> bool {
		Self::get_value(REPUTATION).unwrap_or_default()
	}
}

impl<T: Chainflip> ReputationResetter for MockReputationResetter<T> {
	type ValidatorId = T::ValidatorId;

	fn reset_reputation(_validator: &Self::ValidatorId) {
		Self::put_value(REPUTATION, true);
	}
}
