use crate::*;

pub mod batch_fetch_and_transfer;
pub mod rotate_vault_proxy;

use crate::dot::{CurrentVaultAndProxy, Polkadot, PolkadotReplayProtection};

use super::PolkadotPublicKey;

/// Chainflip api calls available on Polkadot.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum PolkadotApi {
	BatchFetchAndTransfer(batch_fetch_and_transfer::BatchFetchAndTransfer),
	RotateVaultProxy(rotate_vault_proxy::RotateVaultProxy),
}

impl AllBatch<Polkadot> for PolkadotApi {
	fn new_unsigned(
		replay_protection: PolkadotReplayProtection,
		chain_specific_data: CurrentVaultAndProxy,
		fetch_params: Vec<FetchAssetParams<Polkadot>>,
		transfer_params: Vec<TransferAssetParams<Polkadot>>,
	) -> Self {
		Self::BatchFetchAndTransfer(batch_fetch_and_transfer::BatchFetchAndTransfer::new_unsigned(
			replay_protection,
			fetch_params,
			transfer_params,
			chain_specific_data.proxy_account,
			chain_specific_data.vault_account,
		))
	}
}

impl SetAggKeyWithAggKey<Polkadot> for PolkadotApi {
	fn new_unsigned(
		replay_protection: PolkadotReplayProtection,
		chain_specific_data: CurrentVaultAndProxy,
		new_key: PolkadotPublicKey,
	) -> Self {
		Self::RotateVaultProxy(rotate_vault_proxy::RotateVaultProxy::new_unsigned(
			replay_protection,
			new_key,
			chain_specific_data.proxy_account,
			chain_specific_data.vault_account,
		))
	}
}

impl From<batch_fetch_and_transfer::BatchFetchAndTransfer> for PolkadotApi {
	fn from(tx: batch_fetch_and_transfer::BatchFetchAndTransfer) -> Self {
		Self::BatchFetchAndTransfer(tx)
	}
}

impl From<rotate_vault_proxy::RotateVaultProxy> for PolkadotApi {
	fn from(tx: rotate_vault_proxy::RotateVaultProxy) -> Self {
		Self::RotateVaultProxy(tx)
	}
}

impl ApiCall<Polkadot> for PolkadotApi {
	fn threshold_signature_payload(&self) -> <Polkadot as ChainCrypto>::Payload {
		match self {
			PolkadotApi::BatchFetchAndTransfer(tx) => tx.threshold_signature_payload(),
			PolkadotApi::RotateVaultProxy(tx) => tx.threshold_signature_payload(),
		}
	}

	fn signed(self, threshold_signature: &<Polkadot as ChainCrypto>::ThresholdSignature) -> Self {
		match self {
			PolkadotApi::BatchFetchAndTransfer(call) => call.signed(threshold_signature).into(),
			PolkadotApi::RotateVaultProxy(call) => call.signed(threshold_signature).into(),
		}
	}

	fn chain_encoded(&self) -> Vec<u8> {
		match self {
			PolkadotApi::BatchFetchAndTransfer(call) => call.chain_encoded(),
			PolkadotApi::RotateVaultProxy(call) => call.chain_encoded(),
		}
	}

	fn is_signed(&self) -> bool {
		match self {
			PolkadotApi::BatchFetchAndTransfer(call) => call.is_signed(),
			PolkadotApi::RotateVaultProxy(call) => call.is_signed(),
		}
	}
}
