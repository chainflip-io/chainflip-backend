use cf_chains::{ApiCall, Chain};
use cf_primitives::{BroadcastId, ThresholdSignatureRequestId};
use core::marker::PhantomData;
use frame_support::{
	traits::{OriginTrait, UnfilteredDispatchable},
	Parameter,
};
use sp_runtime::traits::Member;

use crate::Broadcaster;

use super::*;

pub struct MockBroadcaster<T>(PhantomData<T>);

impl<T> MockPallet for MockBroadcaster<T> {
	const PREFIX: &'static [u8] = b"MockBroadcaster";
}

impl<
		Api: Chain,
		A: ApiCall<Api::ChainCrypto> + Member + Parameter,
		C: UnfilteredDispatchable + Member + Parameter,
	> Broadcaster<Api> for MockBroadcaster<(A, C)>
{
	type ApiCall = A;
	type Callback = C;

	fn threshold_sign_and_broadcast(
		api_call: Self::ApiCall,
	) -> (cf_primitives::BroadcastId, cf_primitives::ThresholdSignatureRequestId) {
		Self::mutate_value(b"API_CALLS", |api_calls: &mut Option<Vec<A>>| {
			let api_calls = api_calls.get_or_insert(Default::default());
			api_calls.push(api_call);
		});
		(
			<Self as MockPalletStorage>::mutate_value(b"BROADCAST_ID", |v: &mut Option<u32>| {
				let v = v.get_or_insert(0);
				*v += 1;
				*v
			}),
			<Self as MockPalletStorage>::mutate_value(b"THRESHOLD_ID", |v: &mut Option<u32>| {
				let v = v.get_or_insert(0);
				*v += 1;
				*v
			}),
		)
	}

	fn threshold_sign_and_broadcast_with_callback(
		api_call: Self::ApiCall,
		callback: Self::Callback,
	) -> (BroadcastId, ThresholdSignatureRequestId) {
		let ids @ (id, _) = <Self as Broadcaster<Api>>::threshold_sign_and_broadcast(api_call);
		Self::put_storage(b"CALLBACKS", id, callback);
		ids
	}
}

impl<
		A: Decode + 'static,
		O: OriginTrait,
		C: UnfilteredDispatchable<RuntimeOrigin = O> + Member + Parameter,
	> MockBroadcaster<(A, C)>
{
	#[track_caller]
	pub fn dispatch_callback(id: BroadcastId) {
		frame_support::assert_ok!(
			// Use root origin as proxy for witness origin.
			Self::take_storage::<_, C>(b"CALLBACKS", &id)
				.expect("Expected a callback.")
				.dispatch_bypass_filter(OriginTrait::root())
		);
	}

	#[track_caller]
	pub fn dispatch_all_callbacks() {
		for callback in Self::take_pending_callbacks() {
			frame_support::assert_ok!(callback.dispatch_bypass_filter(OriginTrait::root()));
		}
	}

	pub fn get_pending_api_calls() -> Vec<A> {
		Self::get_value(b"API_CALLS").unwrap_or(Default::default())
	}

	pub fn take_pending_callbacks() -> Vec<C> {
		Self::pending_callbacks(Self::take_storage)
	}

	pub fn get_pending_callbacks() -> Vec<C> {
		Self::pending_callbacks(Self::get_storage)
	}

	fn pending_callbacks(mut f: impl FnMut(&[u8], u32) -> Option<C> + 'static) -> Vec<C> {
		let max = Self::get_value(b"BROADCAST_ID").unwrap_or(1);
		(0u32..=max).filter_map(move |id| f(b"CALLBACKS", id)).collect()
	}
}
