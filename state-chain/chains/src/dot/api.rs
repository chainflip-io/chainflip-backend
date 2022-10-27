use crate::*;

pub mod batch_fetch_and_transfer;
pub mod rotate_vault_proxy;

use crate::dot::{Polkadot, PolkadotAccountId, PolkadotReplayProtection};

/// Chainflip api calls available on Polkadot.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum PolkadotApi {
	BatchFetch(batch_fetch_and_transfer::BatchFetchAndTransfer),
}

impl PolkadotBatchFetch for PolkadotApi {
	fn new_unsigned(
		replay_protection: PolkadotReplayProtection,
		fetch_params: Vec<FetchAssetParams<Polkadot>>,
		transfer_params: Vec<TransferAssetParams<Polkadot>>,
		proxy_account: PolkadotAccountId,
		vault_account: PolkadotAccountId,
	) -> Self {
		Self::BatchFetch(batch_fetch_and_transfer::BatchFetchAndTransfer::new_unsigned(
			replay_protection,
			fetch_params,
			transfer_params,
			proxy_account,
			vault_account,
		))
	}
}

pub trait PolkadotBatchFetch: ApiCall<Polkadot> {
	fn new_unsigned(
		replay_protection: <Polkadot as ChainAbi>::ReplayProtection,
		fetch_params: Vec<FetchAssetParams<Polkadot>>,
		transfer_params: Vec<TransferAssetParams<Polkadot>>,
		proxy_account: <Polkadot as Chain>::ChainAccount,
		vault_account: <Polkadot as Chain>::ChainAccount,
	) -> Self;
}

impl From<batch_fetch_and_transfer::BatchFetchAndTransfer> for PolkadotApi {
	fn from(tx: batch_fetch_and_transfer::BatchFetchAndTransfer) -> Self {
		Self::BatchFetch(tx)
	}
}

impl ApiCall<Polkadot> for PolkadotApi {
	fn threshold_signature_payload(&self) -> <Polkadot as ChainCrypto>::Payload {
		match self {
			PolkadotApi::BatchFetch(tx) => tx.threshold_signature_payload(),
		}
	}

	fn signed(self, threshold_signature: &<Polkadot as ChainCrypto>::ThresholdSignature) -> Self {
		match self {
			PolkadotApi::BatchFetch(call) => call.signed(threshold_signature).into(),
		}
	}

	fn chain_encoded(&self) -> <Polkadot as ChainAbi>::SignedTransaction {
		match self {
			PolkadotApi::BatchFetch(call) => call.chain_encoded(),
		}
	}

	fn is_signed(&self) -> bool {
		match self {
			PolkadotApi::BatchFetch(call) => call.is_signed(),
		}
	}
}
