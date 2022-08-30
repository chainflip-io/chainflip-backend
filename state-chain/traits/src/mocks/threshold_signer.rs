use crate::{AsyncResult, CeremonyId, RetryPolicy};

use super::{MockPallet, MockPalletStorage};
use cf_chains::ChainCrypto;
use codec::{Decode, Encode};
use frame_support::{dispatch::UnfilteredDispatchable, traits::OriginTrait};
use std::marker::PhantomData;

pub struct MockThresholdSigner<C, Call>(PhantomData<(C, Call)>);

impl<C, Call> MockPallet for MockThresholdSigner<C, Call> {
	const PREFIX: &'static [u8] = b"MockThresholdSigner::";
}

type MockValidatorId = u64;

const REQUEST: &[u8] = b"REQ";
const LAST_REQ_ID: &[u8] = b"LAST_REQ_ID";
const SIGNATURE: &[u8] = b"SIG";
const CALLBACK: &[u8] = b"CALLBACK";

impl<C, O, Call> MockThresholdSigner<C, Call>
where
	C: ChainCrypto,
	O: OriginTrait,
	Call: UnfilteredDispatchable<Origin = O> + Encode + Decode,
{
	pub fn last_request_id() -> Option<u32> {
		Self::get_value(LAST_REQ_ID)
	}

	pub fn set_signature_ready(
		request_id: u32,
		signature_result: Result<<C as ChainCrypto>::ThresholdSignature, Vec<MockValidatorId>>,
	) {
		Self::put_storage(
			SIGNATURE,
			request_id,
			crate::AsyncResult::<
				Result<<C as ChainCrypto>::ThresholdSignature, Vec<MockValidatorId>>,
			>::Ready(signature_result),
		);
	}

	// Mocks a threshold signing success by inserting a signature and then calls the callback
	pub fn on_signature_ready(request_id: u32) {
		let callback: Call = Self::take_storage(CALLBACK, request_id).unwrap();
		callback.dispatch_bypass_filter(O::root()).expect("Should be valid callback");
	}
}

impl<C, O, Call> crate::ThresholdSigner<C> for MockThresholdSigner<C, Call>
where
	C: ChainCrypto,
	O: OriginTrait,
	Call: UnfilteredDispatchable<Origin = O> + Encode + Decode,
{
	type RequestId = u32;
	type Error = &'static str;
	type Callback = Call;
	type KeyId = Vec<u8>;

	type ValidatorId = MockValidatorId;

	fn request_signature(payload: <C as ChainCrypto>::Payload) -> Self::RequestId {
		let req_id = {
			let payload = payload.clone();
			payload.using_encoded(|bytes| bytes[0]) as u32
		};
		Self::put_storage(
			SIGNATURE,
			req_id,
			AsyncResult::<<C as ChainCrypto>::ThresholdSignature>::Pending,
		);
		Self::put_storage(REQUEST, req_id, payload);
		Self::put_value(LAST_REQ_ID, req_id);
		req_id
	}

	fn register_callback(
		request_id: Self::RequestId,
		on_signature_ready: Self::Callback,
	) -> Result<(), Self::Error> {
		Self::put_storage(CALLBACK, request_id, on_signature_ready);
		Ok(())
	}

	fn signature_result(
		request_id: Self::RequestId,
	) -> crate::AsyncResult<Result<<C as ChainCrypto>::ThresholdSignature, Vec<Self::ValidatorId>>>
	{
		Self::take_storage::<_, AsyncResult<_>>(SIGNATURE, request_id).unwrap_or(AsyncResult::Void)
	}

	fn request_signature_with(
		_key_id: Self::KeyId,
		_participants: Vec<Self::ValidatorId>,
		payload: <C as ChainCrypto>::Payload,
		_retry_policy: RetryPolicy,
	) -> (Self::RequestId, CeremonyId) {
		let req_id = Self::request_signature(payload);
		(req_id, 1)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn insert_signature(request_id: Self::RequestId, signature: C::ThresholdSignature) {
		Self::set_signature_ready(request_id, Ok(signature))
	}
}
