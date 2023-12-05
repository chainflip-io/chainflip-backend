use crate::{BitcoinInstance, Box, EthereumInstance, PolkadotInstance, Runtime, RuntimeCall};
use codec::{Decode, Encode};
use pallet_cf_broadcast::Call;
use scale_info::TypeInfo;
use sp_std::vec::Vec;

use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use pallet_cf_broadcast::{BroadcastAttemptId, RequestSuccessCallbacks};

#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

pub mod old {
	use super::{Decode, Encode, TypeInfo, Vec};
	use cf_chains::{
		btc::{api::BitcoinApi, PreviousOrCurrent, SigningPayload},
		dot::{api::PolkadotApi, EncodedPolkadotPayload},
		eth::api::EthereumApi,
	};
	use cf_primitives::{BroadcastId, ThresholdSignatureRequestId};
	use frame_support::Twox64Concat;
	use sp_core::H256;

	use crate::chainflip::{BtcEnvironment, DotEnvironment, EthEnvironment};

	#[derive(Debug, TypeInfo, Decode, Encode, Clone, PartialEq, Eq)]
	#[allow(non_camel_case_types)]
	pub enum EthereumBroadaster {
		#[codec(index = 1u8)]
		on_signature_ready {
			threshold_request_id: ThresholdSignatureRequestId,
			threshold_signature_payload: H256,
			api_call: EthereumApi<EthEnvironment>,
			broadcast_id: BroadcastId,
			initiated_at: u64,
		},
	}

	#[derive(Debug, TypeInfo, Decode, Encode, Clone, PartialEq, Eq)]
	#[allow(non_camel_case_types)]
	pub enum PolkadotBroadaster {
		#[codec(index = 1u8)]
		on_signature_ready {
			threshold_request_id: ThresholdSignatureRequestId,
			threshold_signature_payload: EncodedPolkadotPayload,
			api_call: PolkadotApi<DotEnvironment>,
			broadcast_id: BroadcastId,
			initiated_at: u32,
		},
	}

	#[derive(Debug, TypeInfo, Decode, Encode, Clone, PartialEq, Eq)]
	#[allow(non_camel_case_types)]
	pub enum BitcoinBroadaster {
		#[codec(index = 1u8)]
		on_signature_ready {
			threshold_request_id: ThresholdSignatureRequestId,
			threshold_signature_payload: Vec<(PreviousOrCurrent, SigningPayload)>,
			api_call: BitcoinApi<BtcEnvironment>,
			broadcast_id: BroadcastId,
			initiated_at: u64,
		},
	}
	#[derive(Debug, Decode, Encode, Clone, PartialEq, Eq, TypeInfo)]
	pub enum RuntimeCall {
		#[codec(index = 27u8)]
		EthereumBroadcaster(EthereumBroadaster),
		#[codec(index = 28u8)]
		PolkadotBroadaster(PolkadotBroadaster),
		#[codec(index = 29u8)]
		BitcoinBroadaster(BitcoinBroadaster),
	}

	impl RuntimeCall {
		pub fn unwrap_eth(
			self,
		) -> (ThresholdSignatureRequestId, H256, EthereumApi<EthEnvironment>, BroadcastId, u64) {
			match self {
				Self::EthereumBroadcaster(b) => match b {
					EthereumBroadaster::on_signature_ready {
						threshold_request_id,
						threshold_signature_payload,
						api_call,
						broadcast_id,
						initiated_at,
					} => (
						threshold_request_id,
						threshold_signature_payload,
						api_call,
						broadcast_id,
						initiated_at,
					),
				},
				_ => unreachable!(),
			}
		}

		pub fn unwrap_dot(
			self,
		) -> (
			ThresholdSignatureRequestId,
			EncodedPolkadotPayload,
			PolkadotApi<DotEnvironment>,
			BroadcastId,
			u32,
		) {
			match self {
				Self::PolkadotBroadaster(b) => match b {
					PolkadotBroadaster::on_signature_ready {
						threshold_request_id,
						threshold_signature_payload,
						api_call,
						broadcast_id,
						initiated_at,
					} => (
						threshold_request_id,
						threshold_signature_payload,
						api_call,
						broadcast_id,
						initiated_at,
					),
				},
				_ => unreachable!(),
			}
		}

		pub fn unwrap_btc(
			self,
		) -> (
			ThresholdSignatureRequestId,
			Vec<(PreviousOrCurrent, SigningPayload)>,
			BitcoinApi<BtcEnvironment>,
			BroadcastId,
			u64,
		) {
			match self {
				Self::BitcoinBroadaster(b) => match b {
					BitcoinBroadaster::on_signature_ready {
						threshold_request_id,
						threshold_signature_payload,
						api_call,
						broadcast_id,
						initiated_at,
					} => (
						threshold_request_id,
						threshold_signature_payload,
						api_call,
						broadcast_id,
						initiated_at,
					),
				},
				_ => unreachable!(),
			}
		}
	}

	#[frame_support::storage_alias]
	pub type RequestCallback<T: pallet_cf_threshold_signature::Config<I>, I: 'static> = StorageMap<
		pallet_cf_threshold_signature::Pallet<T, I>,
		Twox64Concat,
		BroadcastId,
		RuntimeCall,
	>;
}

pub struct ThresholdSignatureCallbacks;
impl OnRuntimeUpgrade for ThresholdSignatureCallbacks {
	fn on_runtime_upgrade() -> Weight {
		use frame_support::storage::StoragePrefixedMap;
		frame_support::migration::move_prefix(
			old::RequestCallback::<Runtime, EthereumInstance>::storage_prefix(),
			RequestSuccessCallbacks::<Runtime, EthereumInstance>::storage_prefix(),
		);
		frame_support::migration::move_prefix(
			old::RequestCallback::<Runtime, BitcoinInstance>::storage_prefix(),
			RequestSuccessCallbacks::<Runtime, BitcoinInstance>::storage_prefix(),
		);
		frame_support::migration::move_prefix(
			old::RequestCallback::<Runtime, PolkadotInstance>::storage_prefix(),
			RequestSuccessCallbacks::<Runtime, PolkadotInstance>::storage_prefix(),
		);

		RequestSuccessCallbacks::<Runtime, EthereumInstance>::translate(
			|_k, v: old::RuntimeCall| {
				let c = v.unwrap_eth();
				Some(RuntimeCall::EthereumBroadcaster(Call::on_signature_ready {
					threshold_request_id: c.0,
					threshold_signature_payload: c.1,
					api_call: Box::new(c.2),
					broadcast_attempt_id: BroadcastAttemptId {
						broadcast_id: c.3,
						attempt_count: 0,
					},
					initiated_at: c.4,
					should_broadcast: true,
				}))
			},
		);

		RequestSuccessCallbacks::<Runtime, PolkadotInstance>::translate(
			|_k, v: old::RuntimeCall| {
				let c = v.unwrap_dot();
				Some(RuntimeCall::PolkadotBroadcaster(Call::on_signature_ready {
					threshold_request_id: c.0,
					threshold_signature_payload: c.1,
					api_call: Box::new(c.2),
					broadcast_attempt_id: BroadcastAttemptId {
						broadcast_id: c.3,
						attempt_count: 0,
					},
					initiated_at: c.4,
					should_broadcast: true,
				}))
			},
		);

		RequestSuccessCallbacks::<Runtime, BitcoinInstance>::translate(
			|_k, v: old::RuntimeCall| {
				let c = v.unwrap_btc();
				Some(RuntimeCall::BitcoinBroadcaster(Call::on_signature_ready {
					threshold_request_id: c.0,
					threshold_signature_payload: c.1,
					api_call: Box::new(c.2),
					broadcast_attempt_id: BroadcastAttemptId {
						broadcast_id: c.3,
						attempt_count: 0,
					},
					initiated_at: c.4,
					should_broadcast: true,
				}))
			},
		);

		Weight::default()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let mut eth_broadcastids = old::RequestCallback::<Runtime, EthereumInstance>::iter()
			.map(|(k, v)| (k, v.unwrap_eth().3))
			.collect::<Vec<(u32, u32)>>();
		let mut dot_broadcastids = old::RequestCallback::<Runtime, PolkadotInstance>::iter()
			.map(|(k, v): (u32, old::RuntimeCall)| (k, v.unwrap_dot().3))
			.collect::<Vec<(u32, u32)>>();
		let mut btc_broadcastids = old::RequestCallback::<Runtime, BitcoinInstance>::iter()
			.map(|(k, v): (u32, old::RuntimeCall)| (k, v.unwrap_btc().3))
			.collect::<Vec<(u32, u32)>>();

		eth_broadcastids.append(&mut dot_broadcastids);
		eth_broadcastids.append(&mut btc_broadcastids);
		Ok(eth_broadcastids.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		use pallet_cf_vaults::ensure_variant;

		let old_storage = <Vec<(u32, u32)>>::decode(&mut &state[..]).unwrap();

		let mut eth_broadcastids = RequestSuccessCallbacks::<Runtime, EthereumInstance>::iter()
			.map(|(k, v)| {
				let call = ensure_variant!(RuntimeCall::EthereumBroadcaster(call) => call, v, DispatchError::Other(".."));
					let broadcast_attempt_id = ensure_variant!(Call::on_signature_ready{broadcast_attempt_id, ..} => broadcast_attempt_id, call, DispatchError::Other(".."));
					assert_eq!(broadcast_attempt_id.attempt_count, 0);
				Ok((k, broadcast_attempt_id.broadcast_id))
			})
			.collect::<Result<Vec<(u32, u32)>, DispatchError>>()?;

		let mut dot_broadcastids = RequestSuccessCallbacks::<Runtime, PolkadotInstance>::iter()
			.map(|(k, v)| {
				let call = ensure_variant!(RuntimeCall::PolkadotBroadcaster(call) => call, v, DispatchError::Other(".."));
					let broadcast_attempt_id = ensure_variant!(Call::on_signature_ready{broadcast_attempt_id, ..} => broadcast_attempt_id, call, DispatchError::Other(".."));
					assert_eq!(broadcast_attempt_id.attempt_count, 0);
				Ok((k, broadcast_attempt_id.broadcast_id))
			})
			.collect::<Result<Vec<(u32, u32)>, DispatchError>>()?;

		let mut btc_broadcastids = RequestSuccessCallbacks::<Runtime, BitcoinInstance>::iter()
			.map(|(k, v)| {
				let call = ensure_variant!(RuntimeCall::BitcoinBroadcaster(call) => call, v, DispatchError::Other(".."));
					let broadcast_attempt_id = ensure_variant!(Call::on_signature_ready{broadcast_attempt_id, ..} => broadcast_attempt_id, call, DispatchError::Other(".."));
					assert_eq!(broadcast_attempt_id.attempt_count, 0);
				Ok((k, broadcast_attempt_id.broadcast_id))
			})
			.collect::<Result<Vec<(u32, u32)>, DispatchError>>()?;

		eth_broadcastids.append(&mut dot_broadcastids);
		eth_broadcastids.append(&mut btc_broadcastids);

		for i in 0..eth_broadcastids.len() {
			assert_eq!(eth_broadcastids[i].0, old_storage[i].0);
			assert_eq!(eth_broadcastids[i].1, old_storage[i].1);
		}

		Ok(())
	}
}
