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

use crate::FetchesTransfersLimitProvider;
use sp_std::cell::RefCell;

thread_local! {
	pub static USE_LIMITS: RefCell<bool> = RefCell::new(false);
}

pub struct MockFetchesTransfersLimitProvider;

impl FetchesTransfersLimitProvider for MockFetchesTransfersLimitProvider {
	fn maybe_transfers_limit() -> Option<usize> {
		if USE_LIMITS.with(|v| *v.borrow()) {
			Some(20)
		} else {
			None
		}
	}

	fn maybe_ccm_limit() -> Option<usize> {
		if USE_LIMITS.with(|v| *v.borrow()) {
			Some(5)
		} else {
			None
		}
	}

	fn maybe_fetches_limit() -> Option<usize> {
		if USE_LIMITS.with(|v| *v.borrow()) {
			Some(20)
		} else {
			None
		}
	}
}

impl MockFetchesTransfersLimitProvider {
	pub fn enable_limits() {
		USE_LIMITS.with(|v| *v.borrow_mut() = true);
	}
}
