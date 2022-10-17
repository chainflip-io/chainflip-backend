use crate::*;

pub mod batch_fetch;

use crate::dot::{Polkadot, PolkadotAccountId, PolkadotReplayProtection};

/// Chainflip api calls available on Ethereum.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum PolkadotApi {
	BatchFetch(batch_fetch::BatchFetch),
}

impl PolkadotBatchFetch for PolkadotApi {
	fn new_unsigned(
		replay_protection: PolkadotReplayProtection,
		intent_ids: Vec<IntentId>,
		vault_account: PolkadotAccountId,
	) -> Self {
		Self::BatchFetch(batch_fetch::BatchFetch::new_unsigned(
			replay_protection,
			intent_ids,
			vault_account,
		))
	}
}

pub trait PolkadotBatchFetch: ApiCall<Polkadot> {
	fn new_unsigned(
		replay_protection: <Polkadot as ChainAbi>::ReplayProtection,
		intent_ids: Vec<IntentId>,
		vault_account: <Polkadot as Chain>::ChainAccount,
	) -> Self;
}

impl From<batch_fetch::BatchFetch> for PolkadotApi {
	fn from(tx: batch_fetch::BatchFetch) -> Self {
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
