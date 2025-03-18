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

use cf_primitives::BlockNumber;
use frame_support::sp_runtime::DispatchError;

use crate::{SwapLimits, SwapLimitsProvider};

pub struct MockSwapLimitsProvider;

impl SwapLimitsProvider for MockSwapLimitsProvider {
	type AccountId = u64;

	fn get_swap_limits() -> SwapLimits {
		SwapLimits {
			max_swap_retry_duration_blocks: 600_u32,
			max_swap_request_duration_blocks: 14400_u32,
		}
	}

	fn validate_refund_params(retry_duration: BlockNumber) -> Result<(), DispatchError> {
		let limits = Self::get_swap_limits();
		if retry_duration > limits.max_swap_retry_duration_blocks {
			return Err(DispatchError::Other("Retry duration too high"));
		}
		Ok(())
	}

	fn validate_dca_params(params: &cf_primitives::DcaParameters) -> Result<(), DispatchError> {
		let limits = Self::get_swap_limits();

		if params.number_of_chunks != 1 {
			if params.number_of_chunks == 0 {
				return Err(DispatchError::Other("Zero number of chunks not allowed"));
			}
			if params.chunk_interval < cf_primitives::SWAP_DELAY_BLOCKS {
				return Err(DispatchError::Other("Chunk interval too low"));
			}
			if let Some(total_swap_request_duration) =
				params.number_of_chunks.saturating_sub(1).checked_mul(params.chunk_interval)
			{
				if total_swap_request_duration > limits.max_swap_request_duration_blocks {
					return Err(DispatchError::Other("Swap request duration too long"));
				}
			} else {
				return Err(DispatchError::Other("Invalid DCA parameters"));
			}
		}
		Ok(())
	}

	fn validate_broker_fees(
		broker_fees: &cf_primitives::Beneficiaries<Self::AccountId>,
	) -> Result<(), DispatchError> {
		let total_bps = broker_fees.iter().fold(0u16, |total, fee| total.saturating_add(fee.bps));

		if total_bps > 1000 {
			return Err(DispatchError::Other("Broker fees too high"));
		};

		Ok(())
	}
}
