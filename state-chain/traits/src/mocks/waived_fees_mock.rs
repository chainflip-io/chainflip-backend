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

#[macro_export]
macro_rules! impl_mock_waived_fees {
	($account_id:ty, $call:ty) => {
		pub struct WaivedFeesMock;

		impl WaivedFees for WaivedFeesMock {
			type AccountId = $account_id;
			type RuntimeCall = $call;
			fn should_waive_fees(call: &Self::RuntimeCall, caller: &Self::AccountId) -> bool {
				false
			}
		}
	};
}
