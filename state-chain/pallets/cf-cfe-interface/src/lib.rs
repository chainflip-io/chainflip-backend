#![cfg_attr(not(feature = "std"), no_std)]

mod benchmarking;
mod weights;

pub use weights::PalletWeight;
use weights::WeightInfo;

use cf_chains::{
	btc::BitcoinCrypto, dot::PolkadotCrypto, evm::EvmCrypto, Bitcoin, Ethereum, Polkadot,
};
use cf_primitives::{Ed25519PublicKey, Ipv6Addr, Port};
use cf_traits::{CfeBroadcastRequest, CfeMultisigRequest, CfePeerRegistration, Chainflip};
use frame_support::{
	pallet_prelude::Hooks,
	storage::types::{StorageValue, ValueQuery},
	traits::StorageVersion,
};
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_std::vec::Vec;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(0);

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
	pub type CfeEvents<T: Config> = StorageValue<_, Vec<CfeEvent<T>>, ValueQuery>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_block_number: BlockNumberFor<T>) -> frame_support::weights::Weight {
			CfeEvents::<T>::kill();

			T::WeightInfo::clear_events()
		}
	}
}

impl<T: Config> CfeMultisigRequest<T, EvmCrypto> for Pallet<T> {
	fn keygen_request(req: KeygenRequest<T>) {
		CfeEvents::<T>::append(CfeEvent::<T>::EthKeygenRequest(req))
	}

	fn signature_request(req: ThresholdSignatureRequest<T, EvmCrypto>) {
		CfeEvents::<T>::append(CfeEvent::<T>::EthThresholdSignatureRequest(req))
	}
}

impl<T: Config> CfeMultisigRequest<T, BitcoinCrypto> for Pallet<T> {
	fn keygen_request(req: KeygenRequest<T>) {
		CfeEvents::<T>::append(CfeEvent::<T>::BtcKeygenRequest(req))
	}

	fn signature_request(req: ThresholdSignatureRequest<T, BitcoinCrypto>) {
		CfeEvents::<T>::append(CfeEvent::<T>::BtcThresholdSignatureRequest(req))
	}

	fn key_handover_request(req: KeyHandoverRequest<T, BitcoinCrypto>) {
		CfeEvents::<T>::append(CfeEvent::<T>::BtcKeyHandoverRequest(req))
	}
}

impl<T: Config> CfeMultisigRequest<T, PolkadotCrypto> for Pallet<T> {
	fn keygen_request(req: KeygenRequest<T>) {
		CfeEvents::<T>::append(CfeEvent::<T>::DotKeygenRequest(req))
	}

	fn signature_request(req: ThresholdSignatureRequest<T, PolkadotCrypto>) {
		CfeEvents::<T>::append(CfeEvent::<T>::DotThresholdSignatureRequest(req))
	}
}

impl<T: Config> CfeBroadcastRequest<T, Polkadot> for Pallet<T> {
	fn tx_broadcast_request(req: TxBroadcastRequest<T, Polkadot>) {
		CfeEvents::<T>::append(CfeEvent::<T>::DotTxBroadcastRequest(req))
	}
}

impl<T: Config> CfeBroadcastRequest<T, Bitcoin> for Pallet<T> {
	fn tx_broadcast_request(req: TxBroadcastRequest<T, Bitcoin>) {
		CfeEvents::<T>::append(CfeEvent::<T>::BtcTxBroadcastRequest(req))
	}
}

impl<T: Config> CfeBroadcastRequest<T, Ethereum> for Pallet<T> {
	fn tx_broadcast_request(req: TxBroadcastRequest<T, Ethereum>) {
		CfeEvents::<T>::append(CfeEvent::<T>::EthTxBroadcastRequest(req))
	}
}

impl<T: Config> CfePeerRegistration<T> for Pallet<T> {
	fn peer_registered(
		account_id: T::ValidatorId,
		pubkey: Ed25519PublicKey,
		port: Port,
		ip: Ipv6Addr,
	) {
		CfeEvents::<T>::append(CfeEvent::<T>::PeerIdRegistered { account_id, pubkey, port, ip })
	}

	fn peer_deregistered(account_id: T::ValidatorId, pubkey: Ed25519PublicKey) {
		CfeEvents::<T>::append(CfeEvent::<T>::PeerIdDeregistered { account_id, pubkey })
	}
}
