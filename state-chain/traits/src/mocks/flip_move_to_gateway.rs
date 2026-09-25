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

use super::{MockPallet, MockPalletStorage};
use crate::MoveFlipToGateway;
use cf_primitives::AssetAmount;

pub struct MockMoveFlipToGateway;

impl MockPallet for MockMoveFlipToGateway {
	const PREFIX: &'static [u8] = b"MockMoveFlipToGateway";
}

const FLIP_TO_BE_SENT_TO_GATEWAY: &[u8] = b"FLIP_TO_BE_SENT_TO_GATEWAY";

impl MockMoveFlipToGateway {
	pub fn set_flip_to_be_sent_to_gateway(flip_to_burn: AssetAmount) {
		Self::put_value(FLIP_TO_BE_SENT_TO_GATEWAY, flip_to_burn);
	}

	pub fn peek_flip_to_be_sent_to_gateway() -> AssetAmount {
		Self::get_value(FLIP_TO_BE_SENT_TO_GATEWAY).unwrap_or_default()
	}
}

impl MoveFlipToGateway for MockMoveFlipToGateway {
	fn add_flip_to_be_sent_to_gateway(amount: AssetAmount) {
		Self::set_flip_to_be_sent_to_gateway(Self::peek_flip_to_be_sent_to_gateway() + amount);
	}
}
