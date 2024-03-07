use crate::*;
use core::marker::PhantomData;
use frame_support::{pallet_prelude::*, traits::OnRuntimeUpgrade};
use sp_std::vec;

pub struct Migration<T: Config>(PhantomData<T>);

mod old {
	use super::*;
	use cfe_events::{
		KeyHandoverRequest, KeygenRequest, ThresholdSignatureRequest, TxBroadcastRequest,
	};

	#[derive(Clone, RuntimeDebug, Encode, Decode, PartialEq, Eq, TypeInfo)]
	pub enum CfeEvent<ValidatorId> {
		EthThresholdSignatureRequest(ThresholdSignatureRequest<ValidatorId, EvmCrypto>),
		DotThresholdSignatureRequest(ThresholdSignatureRequest<ValidatorId, PolkadotCrypto>),
		BtcThresholdSignatureRequest(ThresholdSignatureRequest<ValidatorId, BitcoinCrypto>),
		EthKeygenRequest(KeygenRequest<ValidatorId>),
		DotKeygenRequest(KeygenRequest<ValidatorId>),
		BtcKeygenRequest(KeygenRequest<ValidatorId>),
		BtcKeyHandoverRequest(KeyHandoverRequest<ValidatorId, BitcoinCrypto>),
		EthTxBroadcastRequest(TxBroadcastRequest<ValidatorId, Ethereum>),
		DotTxBroadcastRequest(TxBroadcastRequest<ValidatorId, Polkadot>),
		BtcTxBroadcastRequest(TxBroadcastRequest<ValidatorId, Bitcoin>),
		PeerIdRegistered {
			account_id: ValidatorId,
			pubkey: Ed25519PublicKey,
			port: Port,
			ip: Ipv6Addr,
		},
		PeerIdDeregistered {
			account_id: ValidatorId,
			pubkey: Ed25519PublicKey,
		},
	}
}

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		CfeEvents::<T>::translate(|old_cfe_events| {
			let mut new_cfe_events: Vec<CfeEvent<T>> = vec![];
			old_cfe_events.into_iter().for_each(|old_cfe_event| new_cfe_events.push(match old_cfe_event {
                    old::CfeEvent::<<T as Chainflip>::ValidatorId>::EthThresholdSignatureRequest(sig_request) => CfeEvent::<T>::EvmThresholdSignatureRequest(sig_request),
                    old::CfeEvent::<<T as Chainflip>::ValidatorId>::DotThresholdSignatureRequest(sig_request) => CfeEvent::<T>::DotThresholdSignatureRequest(sig_request),
                    old::CfeEvent::<<T as Chainflip>::ValidatorId>::BtcThresholdSignatureRequest(sig_request) => CfeEvent::<T>::BtcThresholdSignatureRequest(sig_request),
                    old::CfeEvent::<<T as Chainflip>::ValidatorId>::EthKeygenRequest(keygen_request) => CfeEvent::<T>::EvmKeygenRequest(keygen_request),
                    old::CfeEvent::<<T as Chainflip>::ValidatorId>::DotKeygenRequest(keygen_request) => CfeEvent::<T>::DotKeygenRequest(keygen_request),
                    old::CfeEvent::<<T as Chainflip>::ValidatorId>::BtcKeygenRequest(keygen_request) => CfeEvent::<T>::BtcKeygenRequest(keygen_request),
                    old::CfeEvent::<<T as Chainflip>::ValidatorId>::BtcKeyHandoverRequest(handover_request) => CfeEvent::<T>::BtcKeyHandoverRequest(handover_request),
                    old::CfeEvent::<<T as Chainflip>::ValidatorId>::EthTxBroadcastRequest(broadcast_request) => CfeEvent::<T>::EthTxBroadcastRequest(broadcast_request),
                    old::CfeEvent::<<T as Chainflip>::ValidatorId>::DotTxBroadcastRequest(broadcast_request) => CfeEvent::<T>::DotTxBroadcastRequest(broadcast_request),
                    old::CfeEvent::<<T as Chainflip>::ValidatorId>::BtcTxBroadcastRequest(broadcast_request) => CfeEvent::<T>::BtcTxBroadcastRequest(broadcast_request),
                    old::CfeEvent::<<T as Chainflip>::ValidatorId>::PeerIdRegistered {
                        account_id,
                        pubkey,
                        port,
                        ip,
                    } => CfeEvent::<T>::PeerIdRegistered { account_id, pubkey, port, ip },
                    old::CfeEvent::<<T as Chainflip>::ValidatorId>::PeerIdDeregistered { account_id, pubkey } => CfeEvent::<T>::PeerIdDeregistered{account_id, pubkey},
                }));
			Some(new_cfe_events)
		}).expect("vvv");
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(vec![])
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		Ok(())
	}
}
