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
use super::MockPallet;
use crate::{mocks::MockPalletStorage, CcmAdditionalDataHandler};
use cf_chains::ccm_checker::DecodedCcmAdditionalData;
use sp_std::vec::Vec;

pub struct MockCcmAdditionalDataHandler;

impl MockPallet for MockCcmAdditionalDataHandler {
	const PREFIX: &'static [u8] = b"MockCcmAdditionalDataHandler";
}

const CCM_ADDITIONAL_DATA_HANDLER: &[u8] = b"CCM_ADDITIONAL_DATA_HANDLER";

impl MockCcmAdditionalDataHandler {
	pub fn get_data_handled() -> Vec<DecodedCcmAdditionalData> {
		Self::get_value(CCM_ADDITIONAL_DATA_HANDLER).unwrap_or_default()
	}
}

impl CcmAdditionalDataHandler for MockCcmAdditionalDataHandler {
	fn handle_ccm_additional_data(new: DecodedCcmAdditionalData) {
		Self::mutate_value::<Vec<DecodedCcmAdditionalData>, _, _>(
			CCM_ADDITIONAL_DATA_HANDLER,
			|maybe_ccm_data| {
				let ccm_data = maybe_ccm_data.get_or_insert_default();
				ccm_data.push(new);
			},
		);
	}
}
