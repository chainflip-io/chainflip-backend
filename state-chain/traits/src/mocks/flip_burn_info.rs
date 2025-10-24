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
use crate::FlipBurnOrMoveInfo;
use cf_primitives::AssetAmount;

pub struct MockFlipBurnOrMoveInfo;

impl MockPallet for MockFlipBurnOrMoveInfo {
	const PREFIX: &'static [u8] = b"MockFlipBurnOrMoveInfo";
}

const FLIP_TO_BURN: &[u8] = b"FLIP_TO_BURN";
const FLIP_TO_BE_SENT_TO_GATEWAY: &[u8] = b"FLIP_TO_BE_SENT_TO_GATEWAY";
const FLIP_DEFICIT: &[u8] = b"FLIP_DEFICIT";

impl MockFlipBurnOrMoveInfo {
	pub fn set_flip_to_burn(flip_to_burn: AssetAmount) {
		Self::put_value(FLIP_TO_BURN, flip_to_burn);
	}

	pub fn peek_flip_to_burn() -> AssetAmount {
		Self::get_value(FLIP_TO_BURN).unwrap_or_default()
	}
	pub fn set_flip_to_be_sent_to_gateway(flip_to_burn: AssetAmount) {
		Self::put_value(FLIP_TO_BE_SENT_TO_GATEWAY, flip_to_burn);
	}

	pub fn peek_flip_to_be_sent_to_gateway() -> AssetAmount {
		Self::get_value(FLIP_TO_BE_SENT_TO_GATEWAY).unwrap_or_default()
	}
	pub fn set_flip_deficit(flip_to_burn: AssetAmount) {
		Self::put_value(FLIP_DEFICIT, flip_to_burn);
	}

	pub fn peek_flip_deficit() -> AssetAmount {
		Self::get_value(FLIP_DEFICIT).unwrap_or_default()
	}
}

impl FlipBurnOrMoveInfo for MockFlipBurnOrMoveInfo {
	fn take_flip_to_burn() -> AssetAmount {
		Self::take_value(FLIP_TO_BURN).unwrap_or_default()
	}
	fn take_flip_to_be_sent_to_gateway() -> AssetAmount {
		Self::take_value(FLIP_TO_BE_SENT_TO_GATEWAY).unwrap_or_default()
	}
	fn take_flip_deficit() -> AssetAmount {
		Self::take_value(FLIP_DEFICIT).unwrap_or_default()
	}
}
