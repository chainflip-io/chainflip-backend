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
		O: OriginTrait,
		C: UnfilteredDispatchable<RuntimeOrigin = O> + Member + Parameter,
	> Broadcaster<Api> for MockBroadcaster<(A, C)>
{
	type ApiCall = A;
	type Callback = C;

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

	fn threshold_sign_and_broadcast_with_callback(
		api_call: Self::ApiCall,
		success_callback: Option<Self::Callback>,
		failed_callback_generator: impl FnOnce(BroadcastId) -> Option<Self::Callback>,
	) -> BroadcastId {
		let (id, _) = <Self as Broadcaster<Api>>::threshold_sign_and_broadcast(api_call);
		if let Some(callback) = success_callback {
			Self::put_storage(b"SUCCESS_CALLBACKS", id, callback);
		}
		if let Some(callback) = failed_callback_generator(id) {
			Self::put_storage(b"FAILED_CALLBACKS", id, callback);
		}
		id
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

	fn threshold_resign(broadcast_id: BroadcastId) -> Option<ThresholdSignatureRequestId> {
		Self::put_value(b"RESIGNED_CALLBACKS", broadcast_id);
		Some(Self::next_threshold_id())
	}

	/// Clean up storage data related to a broadcast ID.
	fn expire_broadcast(_broadcast_id: BroadcastId) {}

	fn threshold_sign_and_broadcast_rotation_tx(
		api_call: Self::ApiCall,
	) -> (BroadcastId, ThresholdSignatureRequestId) {
		<Self as Broadcaster<Api>>::threshold_sign_and_broadcast(api_call)
	}
}

impl<
		A: Decode + 'static,
		O: OriginTrait,
		C: UnfilteredDispatchable<RuntimeOrigin = O> + Member + Parameter,
	> MockBroadcaster<(A, C)>
{
	#[track_caller]
	pub fn dispatch_success_callback(id: BroadcastId) {
		frame_support::assert_ok!(
			// Use root origin as proxy for witness origin.
			Self::take_storage::<_, C>(b"SUCCESS_CALLBACKS", &id)
				.expect("Expected a callback.")
				.dispatch_bypass_filter(OriginTrait::root())
		);
	}

	#[track_caller]
	pub fn dispatch_failed_callback(id: BroadcastId) {
		frame_support::assert_ok!(
			// Use root origin as proxy for witness origin.
			Self::take_storage::<_, C>(b"FAILED_CALLBACKS", &id)
				.expect("Expected a callback.")
				.dispatch_bypass_filter(OriginTrait::root())
		);
	}

	#[track_caller]
	pub fn dispatch_all_success_callbacks() {
		for callback in Self::take_success_pending_callbacks() {
			frame_support::assert_ok!(callback.dispatch_bypass_filter(OriginTrait::root()));
		}
	}

	#[track_caller]
	pub fn dispatch_all_failed_callbacks() {
		for callback in Self::take_failed_pending_callbacks() {
			frame_support::assert_ok!(callback.dispatch_bypass_filter(OriginTrait::root()));
		}
	}

	pub fn get_pending_api_calls() -> Vec<A> {
		Self::get_value(b"API_CALLS").unwrap_or(Default::default())
	}

	pub fn take_success_pending_callbacks() -> Vec<C> {
		Self::pending_success_callbacks(Self::take_storage)
	}
	pub fn take_failed_pending_callbacks() -> Vec<C> {
		Self::pending_failed_callbacks(Self::take_storage)
	}

	pub fn get_success_pending_callbacks() -> Vec<C> {
		Self::pending_success_callbacks(Self::get_storage)
	}
	pub fn get_failed_pending_callbacks() -> Vec<C> {
		Self::pending_failed_callbacks(Self::get_storage)
	}

	pub fn pending_success_callbacks(
		mut f: impl FnMut(&[u8], u32) -> Option<C> + 'static,
	) -> Vec<C> {
		let max = Self::get_value(b"BROADCAST_ID").unwrap_or(1);
		(0u32..=max).filter_map(move |id| f(b"SUCCESS_CALLBACKS", id)).collect()
	}

	pub fn pending_failed_callbacks(
		mut f: impl FnMut(&[u8], u32) -> Option<C> + 'static,
	) -> Vec<C> {
		let max = Self::get_value(b"BROADCAST_ID").unwrap_or(1);
		(0u32..=max).filter_map(move |id| f(b"FAILED_CALLBACKS", id)).collect()
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
