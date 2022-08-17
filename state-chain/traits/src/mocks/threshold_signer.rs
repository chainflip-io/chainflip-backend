use crate::{AsyncResult, RetryPolicy};

use super::{MockPallet, MockPalletStorage};
use cf_chains::ChainCrypto;
use codec::{Decode, Encode};
use frame_support::{dispatch::UnfilteredDispatchable, traits::OriginTrait};
use std::marker::PhantomData;

pub struct MockThresholdSigner<C, Call>(PhantomData<(C, Call)>);

impl<C, Call> MockPallet for MockThresholdSigner<C, Call> {
	const PREFIX: &'static [u8] = b"MockThresholdSigner::";
}

impl<C, O, Call> MockThresholdSigner<C, Call>
where
	C: ChainCrypto,
	O: OriginTrait,
	Call: UnfilteredDispatchable<Origin = O> + Encode + Decode,
{
	pub fn threshold_signature_ready(request_id: u32, sig: <C as ChainCrypto>::ThresholdSignature) {
		Self::put_storage(b"SIG", request_id, AsyncResult::Ready(sig));
		Self::get_storage::<_, Call>(b"CALLBACK", request_id)
			.map(|c| c.dispatch_bypass_filter(O::none()));
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
	type KeyId = u32;

	type ValidatorId = u64;

	fn request_signature(payload: <C as ChainCrypto>::Payload) -> Self::RequestId {
		let id = payload.using_encoded(|bytes| bytes[0]) as u32;
		Self::put_storage(
			b"SIG",
			id,
			AsyncResult::<<C as ChainCrypto>::ThresholdSignature>::Pending,
		);
		Self::put_storage(b"REQ", id, payload);
		id
	}

	fn register_callback(
		request_id: Self::RequestId,
		on_signature_ready: Self::Callback,
	) -> Result<(), Self::Error> {
		Self::put_storage(b"CALLBACK", request_id, on_signature_ready);
		Ok(())
	}

	fn signature_result(
		request_id: Self::RequestId,
	) -> crate::AsyncResult<<C as ChainCrypto>::ThresholdSignature> {
		Self::take_storage::<_, AsyncResult<_>>(b"SIG", request_id).unwrap_or(AsyncResult::Void)
	}

	fn request_signature_with(
		_key_id: Self::KeyId,
		_participants: Vec<Self::ValidatorId>,
		_payload: <C as ChainCrypto>::Payload,
		_retry_policy: RetryPolicy,
	) -> Self::RequestId {
		todo!()
	}
}
