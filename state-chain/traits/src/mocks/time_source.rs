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

use std::{cell::RefCell, ops::AddAssign, time::Duration};

pub struct Mock;

const START: Duration = Duration::from_secs(0);

thread_local! {
	pub static FAKE_TIME: RefCell<Duration> = const { RefCell::new(START) };
}

impl Mock {
	pub fn tick(duration: Duration) {
		FAKE_TIME.with(|cell| cell.borrow_mut().add_assign(duration));
	}

	pub fn reset_to(start: Duration) {
		FAKE_TIME.with(|cell| *cell.borrow_mut() = start);
	}
}

impl frame_support::traits::UnixTime for Mock {
	fn now() -> Duration {
		FAKE_TIME.with(|cell| *cell.borrow())
	}
}
