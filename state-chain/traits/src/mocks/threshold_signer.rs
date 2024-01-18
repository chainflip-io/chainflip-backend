use crate::AsyncResult;

use super::{MockPallet, MockPalletStorage};
use cf_chains::ChainCrypto;
use cf_primitives::{EpochIndex, ThresholdSignatureRequestId};
use codec::{Decode, Encode};
use frame_support::traits::{OriginTrait, UnfilteredDispatchable};
use sp_std::collections::btree_set::BTreeSet;
use std::marker::PhantomData;

pub struct MockThresholdSigner<C, Call>(PhantomData<(C, Call)>);

impl<C, Call> MockPallet for MockThresholdSigner<C, Call> {
	const PREFIX: &'static [u8] = b"MockThresholdSigner::";
}

type MockValidatorId = u64;

const REQUEST: &[u8] = b"REQ";
const KEY_VERIFICATION_REQUEST: &[u8] = b"VERIFICATION_REQ";
const LAST_REQ_ID: &[u8] = b"LAST_REQ_ID";
const SIGNATURE: &[u8] = b"SIG";
const CALLBACK: &[u8] = b"CALLBACK";

#[derive(Encode, Decode, Debug, PartialEq, Eq)]
pub struct VerificationParams<C: ChainCrypto> {
	pub participants: BTreeSet<MockValidatorId>,
	pub key: <C as ChainCrypto>::AggKey,
	pub epoch_index: EpochIndex,
}

impl<C, O, Call> MockThresholdSigner<C, Call>
where
	C: ChainCrypto,
	O: OriginTrait,
	Call: UnfilteredDispatchable<RuntimeOrigin = O> + Encode + Decode,
{
	pub fn last_request_id() -> Option<u32> {
		Self::get_value(LAST_REQ_ID)
	}

	pub fn last_key_verification_request() -> Option<VerificationParams<C>> {
		Self::get_value(KEY_VERIFICATION_REQUEST)
	}

	pub fn execute_signature_result_against_last_request(
		signature_result: Result<<C as ChainCrypto>::ThresholdSignature, Vec<MockValidatorId>>,
	) {
		let last_request_id = Self::last_request_id().unwrap();
		Self::set_signature_ready(last_request_id, signature_result);
		Self::on_signature_ready(last_request_id);
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
	Call: UnfilteredDispatchable<RuntimeOrigin = O> + Encode + Decode,
{
	type Error = &'static str;
	type Callback = Call;

	type ValidatorId = MockValidatorId;

	fn request_signature(payload: <C as ChainCrypto>::Payload) -> ThresholdSignatureRequestId {
		let req_id = payload.using_encoded(|bytes| bytes[0]) as u32;
		Self::put_storage(
			SIGNATURE,
			req_id,
			AsyncResult::<<C as ChainCrypto>::ThresholdSignature>::Pending,
		);
		Self::put_storage(REQUEST, req_id, payload);
		Self::put_value(LAST_REQ_ID, req_id);
		req_id
	}

	fn request_verification_signature(
		payload: <C as ChainCrypto>::Payload,
		participants: BTreeSet<Self::ValidatorId>,
		key: <C as ChainCrypto>::AggKey,
		epoch_index: EpochIndex,
		on_signature_ready: impl FnOnce(ThresholdSignatureRequestId) -> Self::Callback,
	) -> ThresholdSignatureRequestId {
		Self::put_value(
			KEY_VERIFICATION_REQUEST,
			VerificationParams::<C> { participants, key, epoch_index },
		);
		let req_id = Self::request_signature(payload);
		Self::register_callback(req_id, on_signature_ready(req_id)).unwrap();
		req_id
	}

	fn register_callback(
		request_id: ThresholdSignatureRequestId,
		on_signature_ready: Self::Callback,
	) -> Result<(), Self::Error> {
		Self::put_storage(CALLBACK, request_id, on_signature_ready);
		Ok(())
	}

	fn signature_result(
		request_id: ThresholdSignatureRequestId,
	) -> crate::AsyncResult<Result<<C as ChainCrypto>::ThresholdSignature, Vec<Self::ValidatorId>>>
	{
		Self::take_storage::<_, AsyncResult<_>>(SIGNATURE, request_id).unwrap_or(AsyncResult::Void)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn insert_signature(request_id: ThresholdSignatureRequestId, signature: C::ThresholdSignature) {
		Self::set_signature_ready(request_id, Ok(signature))
	}
}
