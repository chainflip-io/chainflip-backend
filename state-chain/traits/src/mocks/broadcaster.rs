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

use cf_chains::{ApiCall, Chain, ChainCrypto};
use cf_primitives::{BroadcastId, ThresholdSignatureRequestId};
use codec::{DecodeWithMemTracking, MaxEncodedLen};
use frame_support::{
	sp_runtime::{traits::Member, DispatchError},
	CloneNoBound, DebugNoBound, DefaultNoBound, EqNoBound, Parameter, PartialEqNoBound,
};
use scale_info::TypeInfo;
use sp_std::{marker::PhantomData, vec::Vec};

use crate::Broadcaster;

use super::*;

pub struct MockBroadcaster<T>(PhantomData<T>);

impl<T> MockPallet for MockBroadcaster<T> {
	const PREFIX: &'static [u8] = b"MockBroadcaster";
}

#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	CloneNoBound,
	Copy,
	DefaultNoBound,
	TypeInfo,
	PartialEqNoBound,
	EqNoBound,
	DebugNoBound,
	MaxEncodedLen,
)]
#[scale_info(skip_type_params(C))]
pub struct MockApiCall<C> {
	is_signed: bool,
	_phantom: PhantomData<C>,
}

impl<C: ChainCrypto + 'static> ApiCall<C> for MockApiCall<C> {
	fn threshold_signature_payload(&self) -> <C as cf_chains::ChainCrypto>::Payload {
		unimplemented!()
	}

	fn signed(
		self,
		_threshold_signature: &<C as cf_chains::ChainCrypto>::ThresholdSignature,
		_signer: <C as cf_chains::ChainCrypto>::AggKey,
	) -> Self {
		Self { is_signed: true, _phantom: Default::default() }
	}

	fn chain_encoded(&self) -> Vec<u8> {
		self.encode()
	}

	fn is_signed(&self) -> bool {
		self.is_signed
	}

	fn transaction_out_id(&self) -> <C as cf_chains::ChainCrypto>::TransactionOutId {
		unimplemented!()
	}

	fn refresh_replay_protection(&mut self) {
		unimplemented!()
	}

	fn signer(&self) -> Option<<C as ChainCrypto>::AggKey> {
		unimplemented!()
	}
}

impl<Api: Chain, A: ApiCall<Api::ChainCrypto> + Member + Parameter> Broadcaster<Api>
	for MockBroadcaster<A>
{
	type ApiCall = A;

	fn threshold_sign_and_broadcast(
		api_call: Self::ApiCall,
	) -> (cf_primitives::BroadcastId, ThresholdSignatureRequestId) {
		Self::mutate_value(b"API_CALLS", |api_calls: &mut Option<Vec<A>>| {
			let api_calls = api_calls.get_or_insert(Default::default());
			api_calls.push(api_call);
		});
		let tss_request_id = Self::next_threshold_id();
		(
			<Self as MockPalletStorage>::mutate_value(b"BROADCAST_ID", |v: &mut Option<u32>| {
				let v = v.get_or_insert(0);
				*v += 1;
				*v
			}),
			tss_request_id,
		)
	}

	fn threshold_sign(_api_call: Self::ApiCall) -> (BroadcastId, ThresholdSignatureRequestId) {
		(
			<Self as MockPalletStorage>::mutate_value(b"BROADCAST_ID", |v: &mut Option<u32>| {
				let v = v.get_or_insert(0);
				*v += 1;
				*v
			}),
			Self::next_threshold_id(),
		)
	}

	fn re_sign_broadcast(
		broadcast_id: BroadcastId,
		_request_broadcast: bool,
		_refresh_replay_protection: bool,
	) -> Result<ThresholdSignatureRequestId, DispatchError> {
		Self::put_value(b"RESIGNED_CALLBACKS", broadcast_id);
		Ok(Self::next_threshold_id())
	}

	fn expire_broadcast(_broadcast_id: BroadcastId) {}

	fn threshold_sign_and_broadcast_rotation_tx(
		api_call: Self::ApiCall,
		_key: <<Api as Chain>::ChainCrypto as ChainCrypto>::AggKey,
	) -> (BroadcastId, ThresholdSignatureRequestId) {
		<Self as Broadcaster<Api>>::threshold_sign_and_broadcast(api_call)
	}
}

impl<A: Decode + 'static> MockBroadcaster<A> {
	pub fn get_pending_api_calls() -> Vec<A> {
		Self::get_value(b"API_CALLS").unwrap_or(Default::default())
	}

	fn next_threshold_id() -> ThresholdSignatureRequestId {
		<Self as MockPalletStorage>::mutate_value(b"THRESHOLD_ID", |v: &mut Option<u32>| {
			let v = v.get_or_insert(0);
			*v += 1;
			*v
		})
	}

	pub fn resigned_call() -> Option<ThresholdSignatureRequestId> {
		Self::get_value(b"RESIGNED_CALLBACKS")
	}
}
