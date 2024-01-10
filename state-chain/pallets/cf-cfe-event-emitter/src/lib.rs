#![cfg_attr(not(feature = "std"), no_std)]

mod benchmarking;
mod weights;

pub use weights::PalletWeight;
use weights::WeightInfo;

use cf_chains::{
	btc::BitcoinCrypto, dot::PolkadotCrypto, evm::EvmCrypto, Bitcoin, Ethereum, Polkadot,
};
use cf_primitives::{Ed25519PublicKey, Ipv6Addr, Port};
use cf_traits::{CfeEventEmitterForChain, CfeEventEmitterForCrypto, CfeEventEmitterT, Chainflip};
use frame_support::{
	pallet_prelude::Hooks, storage::types::StorageMap, traits::StorageVersion, Twox64Concat,
};
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_std::vec::Vec;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(1);

/// How long to keep cfe events for (in SC blocks)
const EVENT_LIFETIME: u32 = 20;

pub type EventId = u64;

pub type CfeEvent<T> = cfe_events::CfeEvent<<T as Chainflip>::ValidatorId>;
pub type ThresholdSignatureRequest<T, C> =
	cfe_events::ThresholdSignatureRequest<<T as Chainflip>::ValidatorId, C>;
pub type KeyHandoverRequest<T, C> =
	cfe_events::KeyHandoverRequest<<T as Chainflip>::ValidatorId, C>;
pub type KeygenRequest<T> = cfe_events::KeygenRequest<<T as Chainflip>::ValidatorId>;
pub type TxBroadcastRequest<T, C> =
	cfe_events::TxBroadcastRequest<<T as Chainflip>::ValidatorId, C>;

#[frame_support::pallet]
pub mod pallet {

	use cf_traits::Chainflip;

	use super::*;

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		type WeightInfo: WeightInfo;
	}

	#[pallet::storage]
	#[pallet::getter(fn get_cfe_events)]
	#[pallet::unbounded]
	pub type CfeEvents<T: Config> =
		StorageMap<_, Twox64Concat, BlockNumberFor<T>, Vec<CfeEvent<T>>>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(block_number: BlockNumberFor<T>) -> frame_support::weights::Weight {
			use frame_support::sp_runtime::Saturating;

			CfeEvents::<T>::remove(block_number.saturating_sub(EVENT_LIFETIME.into()));

			T::WeightInfo::remove_events_for_block()
		}
	}
}

fn add_event<T: Config>(event: CfeEvent<T>) {
	let number = frame_system::Pallet::<T>::block_number();

	CfeEvents::<T>::mutate(number, |events| {
		let events = events.get_or_insert(Vec::new());
		events.push(event);
	})
}

impl<T: Config> CfeEventEmitterForCrypto<T, EvmCrypto> for Pallet<T> {
	fn keygen_request(req: KeygenRequest<T>) {
		add_event::<T>(CfeEvent::<T>::EthKeygenRequest(req));
	}

	fn signature_request(req: ThresholdSignatureRequest<T, EvmCrypto>) {
		add_event::<T>(CfeEvent::<T>::EthThresholdSignatureRequest(req));
	}
}

impl<T: Config> CfeEventEmitterForCrypto<T, BitcoinCrypto> for Pallet<T> {
	fn keygen_request(req: KeygenRequest<T>) {
		add_event::<T>(CfeEvent::<T>::BtcKeygenRequest(req));
	}

	fn signature_request(req: ThresholdSignatureRequest<T, BitcoinCrypto>) {
		add_event::<T>(CfeEvent::<T>::BtcThresholdSignatureRequest(req));
	}

	fn key_handover_request(req: KeyHandoverRequest<T, BitcoinCrypto>) {
		add_event::<T>(CfeEvent::<T>::BtcKeyHandoverRequest(req))
	}
}

impl<T: Config> CfeEventEmitterForCrypto<T, PolkadotCrypto> for Pallet<T> {
	fn keygen_request(req: KeygenRequest<T>) {
		add_event::<T>(CfeEvent::<T>::DotKeygenRequest(req));
	}

	fn signature_request(req: ThresholdSignatureRequest<T, PolkadotCrypto>) {
		add_event::<T>(CfeEvent::<T>::DotThresholdSignatureRequest(req));
	}
}

impl<T: Config> CfeEventEmitterForChain<T, Polkadot> for Pallet<T> {
	fn tx_broadcast_request(req: TxBroadcastRequest<T, Polkadot>) {
		add_event::<T>(CfeEvent::<T>::DotTxBroadcastRequest(req));
	}
}

impl<T: Config> CfeEventEmitterForChain<T, Bitcoin> for Pallet<T> {
	fn tx_broadcast_request(req: TxBroadcastRequest<T, Bitcoin>) {
		add_event::<T>(CfeEvent::<T>::BtcTxBroadcastRequest(req));
	}
}

impl<T: Config> CfeEventEmitterForChain<T, Ethereum> for Pallet<T> {
	fn tx_broadcast_request(req: TxBroadcastRequest<T, Ethereum>) {
		add_event::<T>(CfeEvent::<T>::EthTxBroadcastRequest(req));
	}
}

impl<T: Config> CfeEventEmitterT<T> for Pallet<T> {
	fn peer_registered(
		account_id: T::ValidatorId,
		pubkey: Ed25519PublicKey,
		port: Port,
		ip: Ipv6Addr,
	) {
		add_event::<T>(CfeEvent::<T>::PeerIdRegistered { account_id, pubkey, port, ip })
	}

	fn peer_deregistered(account_id: T::ValidatorId, pubkey: Ed25519PublicKey) {
		add_event::<T>(CfeEvent::<T>::PeerIdDeregistered { account_id, pubkey })
	}
}
