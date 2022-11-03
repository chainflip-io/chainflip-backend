pub mod batch_fetch_and_transfer;
pub mod rotate_vault_proxy;

use super::{PolkadotAccountId, PolkadotPublicKey};
use crate::{
	dot::{Polkadot, PolkadotReplayProtection},
	*,
};
use frame_support::Never;
use sp_std::marker::PhantomData;

/// Chainflip api calls available on Polkadot.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum PolkadotApi<Environment: 'static> {
	BatchFetchAndTransfer(batch_fetch_and_transfer::BatchFetchAndTransfer),
	RotateVaultProxy(rotate_vault_proxy::RotateVaultProxy),
	#[doc(hidden)]
	#[codec(skip)]
	_Phantom(PhantomData<Environment>, Never),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum SystemAccounts {
	Proxy,
	Vault,
}

impl<E> AllBatch<Polkadot> for PolkadotApi<E>
where
	E: ChainEnvironment<SystemAccounts, <Polkadot as Chain>::ChainAccount>,
{
	fn new_unsigned(
		replay_protection: PolkadotReplayProtection,
		fetch_params: Vec<FetchAssetParams<Polkadot>>,
		transfer_params: Vec<TransferAssetParams<Polkadot>>,
	) -> Self {
		Self::BatchFetchAndTransfer(batch_fetch_and_transfer::BatchFetchAndTransfer::new_unsigned(
			replay_protection,
			fetch_params,
			transfer_params,
			E::lookup(SystemAccounts::Proxy),
			E::lookup(SystemAccounts::Vault),
		))
	}
}

impl<E> SetAggKeyWithAggKey<Polkadot> for PolkadotApi<E>
where
	E: ChainEnvironment<SystemAccounts, <Polkadot as Chain>::ChainAccount>,
{
	fn new_unsigned(
		replay_protection: PolkadotReplayProtection,
		old_key: PolkadotPublicKey,
		new_key: PolkadotPublicKey,
	) -> Self {
		Self::RotateVaultProxy(rotate_vault_proxy::RotateVaultProxy::new_unsigned(
			replay_protection,
			new_key,
			old_key,
			E::lookup(SystemAccounts::Proxy),
			E::lookup(SystemAccounts::Vault),
		))
	}
}

impl<E> From<batch_fetch_and_transfer::BatchFetchAndTransfer> for PolkadotApi<E> {
	fn from(tx: batch_fetch_and_transfer::BatchFetchAndTransfer) -> Self {
		Self::BatchFetchAndTransfer(tx)
	}
}

impl<E> From<rotate_vault_proxy::RotateVaultProxy> for PolkadotApi<E> {
	fn from(tx: rotate_vault_proxy::RotateVaultProxy) -> Self {
		Self::RotateVaultProxy(tx)
	}
}

impl<E> ApiCall<Polkadot> for PolkadotApi<E> {
	fn threshold_signature_payload(&self) -> <Polkadot as ChainCrypto>::Payload {
		match self {
			PolkadotApi::BatchFetchAndTransfer(tx) => tx.threshold_signature_payload(),
			PolkadotApi::RotateVaultProxy(tx) => tx.threshold_signature_payload(),
			PolkadotApi::_Phantom(..) => unreachable!(),
		}
	}

	fn signed(self, threshold_signature: &<Polkadot as ChainCrypto>::ThresholdSignature) -> Self {
		match self {
			PolkadotApi::BatchFetchAndTransfer(call) => call.signed(threshold_signature).into(),
			PolkadotApi::RotateVaultProxy(call) => call.signed(threshold_signature).into(),
			PolkadotApi::_Phantom(..) => unreachable!(),
		}
	}

	fn chain_encoded(&self) -> Vec<u8> {
		match self {
			PolkadotApi::BatchFetchAndTransfer(call) => call.chain_encoded(),
			PolkadotApi::RotateVaultProxy(call) => call.chain_encoded(),
			PolkadotApi::_Phantom(..) => unreachable!(),
		}
	}

	fn is_signed(&self) -> bool {
		match self {
			PolkadotApi::BatchFetchAndTransfer(call) => call.is_signed(),
			PolkadotApi::RotateVaultProxy(call) => call.is_signed(),
			PolkadotApi::_Phantom(..) => unreachable!(),
		}
	}
}
