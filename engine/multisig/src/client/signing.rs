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

mod signing_data;
mod signing_detail;
mod signing_stages;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use crate::CryptoScheme;

use super::common::KeygenResult;

pub use signing_data::{
	Comm1, LocalSig3, LocalSig3Inner, SigningCommitment, SigningData, VerifyComm2, VerifyLocalSig4,
};

pub use signing_detail::generate_schnorr_response;

pub use signing_stages::AwaitCommitments1;

#[cfg(test)]
pub use signing_data::{gen_signing_data_stage1, gen_signing_data_stage2, gen_signing_data_stage4};

pub use signing_detail::get_lagrange_coeff;

/// Payload and the key that should be used to sign over the payload
pub struct PayloadAndKey<C: CryptoScheme> {
	pub payload: C::SigningPayload,
	pub key: Arc<KeygenResult<C>>,
}

/// Data common for signing stages
pub struct SigningStateCommonInfo<C: CryptoScheme> {
	pub payloads_and_keys: Vec<PayloadAndKey<C>>,
}

impl<C: CryptoScheme> SigningStateCommonInfo<C> {
	pub fn payload_count(&self) -> usize {
		self.payloads_and_keys.len()
	}
}
