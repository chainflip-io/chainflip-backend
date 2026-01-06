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

use crate::{
	mocks::{MockPallet, MockPalletStorage},
	WaivedFees,
};

pub struct WaivedFeesMock<T>(sp_std::marker::PhantomData<T>);

impl<T: frame_system::Config> MockPallet for WaivedFeesMock<T> {
	const PREFIX: &'static [u8] = b"WaivedFeesMock";
}

impl<T: frame_system::Config> WaivedFees for WaivedFeesMock<T> {
	type AccountId = T::AccountId;
	type RuntimeCall = T::RuntimeCall;

	fn should_waive_fees(_call: &Self::RuntimeCall, _caller: &Self::AccountId) -> bool {
		Self::get_value::<bool>(b"SHOULD_WAIVE_FEES").unwrap_or(false)
	}
}

impl<T: frame_system::Config> WaivedFeesMock<T> {
	pub fn set_should_waive_fees(should_waive: bool) {
		Self::put_value(b"SHOULD_WAIVE_FEES", should_waive);
	}
}
