//! Types and functions that are common to ethereum.
pub mod api;

pub mod benchmarking;

pub mod deposit_address;

use crate::{
	evm::{DeploymentStatus, EvmFetchId, Transaction},
	*,
};
use cf_primitives::chains::assets;
pub use cf_primitives::chains::Ethereum;
use codec::{Decode, Encode, MaxEncodedLen};
pub use ethabi::{
	ethereum_types::{H256, U256},
	Address, Hash as TxHash, Token, Uint, Word,
};
use evm::api::EvmReplayProtection;
use frame_support::sp_runtime::{FixedPointNumber, FixedU64, RuntimeDebug};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::{cmp::min, convert::TryInto, str};

// Reference constants for the chain spec
pub const CHAIN_ID_MAINNET: u64 = 1;
pub const CHAIN_ID_ROPSTEN: u64 = 3;
pub const CHAIN_ID_GOERLI: u64 = 5;
pub const CHAIN_ID_KOVAN: u64 = 42;

impl Chain for Ethereum {
	const NAME: &'static str = "Ethereum";
	type ChainCrypto = evm::EvmCrypto;

	type ChainBlockNumber = u64;
	type ChainAmount = EthAmount;
	type TransactionFee = evm::TransactionFee;
	type TrackedData = EthereumTrackedData;
	type ChainAccount = evm::Address;
	type ChainAsset = assets::eth::Asset;
	type EpochStartData = ();
	type DepositFetchId = EvmFetchId;
	type DepositChannelState = DeploymentStatus;
	type DepositDetails = ();
	type Transaction = Transaction;
	type ReplayProtectionParams = Self::ChainAccount;
	type ReplayProtection = EvmReplayProtection;
}
#[derive(
	Copy,
	Clone,
	RuntimeDebug,
	PartialEq,
	Eq,
	Encode,
	Decode,
	MaxEncodedLen,
	TypeInfo,
	Serialize,
	Deserialize,
)]
#[codec(mel_bound())]
pub struct EthereumTrackedData {
	pub base_fee: <Ethereum as Chain>::ChainAmount,
	pub priority_fee: <Ethereum as Chain>::ChainAmount,
}

pub struct EthereumTransactionValidator;

impl TransactionValidator for EthereumTransactionValidator {
	type Transaction = Transaction;
	type Signature = H256;

	fn is_valid(transaction: Self::Transaction, signature: Self::Signature) -> bool {
		true
	}
}

impl EthereumTrackedData {
	pub fn max_fee_per_gas(
		&self,
		base_fee_multiplier: FixedU64,
	) -> <Ethereum as Chain>::ChainAmount {
		base_fee_multiplier
			.saturating_mul_int(self.base_fee)
			.saturating_add(self.priority_fee)
	}
}

impl Default for EthereumTrackedData {
	#[track_caller]
	fn default() -> Self {
		panic!("You should not use the default chain tracking, as it's meaningless.")
	}
}

impl FeeRefundCalculator<Ethereum> for Transaction {
	fn return_fee_refund(
		&self,
		fee_paid: <Ethereum as Chain>::TransactionFee,
	) -> <Ethereum as Chain>::ChainAmount {
		min(
			self.max_fee_per_gas
				.unwrap_or_default()
				.try_into()
				.expect("In practice `max_fee_per_gas` is always less than u128::MAX"),
			fee_paid.effective_gas_price,
		)
		.saturating_mul(fee_paid.gas_used)
	}
}

impl From<&DepositChannel<Ethereum>> for EvmFetchId {
	fn from(channel: &DepositChannel<Ethereum>) -> Self {
		match channel.state {
			DeploymentStatus::Undeployed => EvmFetchId::DeployAndFetch(channel.channel_id),
			DeploymentStatus::Pending | DeploymentStatus::Deployed =>
				if channel.asset == assets::eth::Asset::Eth {
					EvmFetchId::NotRequired
				} else {
					EvmFetchId::Fetch(channel.address)
				},
		}
	}
}

#[cfg(any(test, feature = "runtime-benchmarks"))]
pub mod sig_constants {
	/*
		The below constants have been derived from integration tests with the KeyManager contract.

		In order to check if verification works, we need to use this to construct the AggKey and `SigData` as we
		normally would when submitting a function call to a threshold-signature-protected smart contract.
	*/
	pub const AGG_KEY_PRIV: [u8; 32] =
		hex_literal::hex!("fbcb47bc85b881e0dfb31c872d4e06848f80530ccbd18fc016a27c4a744d0eba");
	pub const AGG_KEY_PUB: [u8; 33] =
		hex_literal::hex!("0331b2ba4b46201610901c5164f42edd1f64ce88076fde2e2c544f9dc3d7b350ae");
	pub const MSG_HASH: [u8; 32] =
		hex_literal::hex!("2bdc19071c7994f088103dbf8d5476d6deb6d55ee005a2f510dc7640055cc84e");
	pub const SIG: [u8; 32] =
		hex_literal::hex!("beb37e87509e15cd88b19fa224441c56acc0e143cb25b9fd1e57fdafed215538");
	pub const SIG_NONCE: [u8; 32] =
		hex_literal::hex!("d51e13c68bf56155a83e50fd9bc840e2a1847fb9b49cd206a577ecd1cd15e285");
}

#[cfg(test)]
mod lifecycle_tests {
	use super::*;
	const ETH: assets::eth::Asset = assets::eth::Asset::Eth;
	const USDC: assets::eth::Asset = assets::eth::Asset::Usdc;

	macro_rules! expect_deposit_state {
		( $state:expr, $asset:expr, $pat:pat ) => {
			assert!(matches!(
				DepositChannel::<Ethereum> {
					channel_id: Default::default(),
					address: Default::default(),
					asset: $asset,
					state: $state,
				}
				.fetch_id(),
				$pat
			));
		};
	}
	#[test]
	fn eth_deposit_address_lifecycle() {
		// Initial state is undeployed.
		let mut state = DeploymentStatus::default();
		assert_eq!(state, DeploymentStatus::Undeployed);
		assert!(state.can_fetch());
		expect_deposit_state!(state, ETH, EvmFetchId::DeployAndFetch(..));
		expect_deposit_state!(state, USDC, EvmFetchId::DeployAndFetch(..));

		// Pending channels can't be fetched from.
		assert!(state.on_fetch_scheduled());
		assert_eq!(state, DeploymentStatus::Pending);
		assert!(!state.can_fetch());

		// Trying to schedule the fetch on a pending channel has no effect.
		assert!(!state.on_fetch_scheduled());
		assert_eq!(state, DeploymentStatus::Pending);
		assert!(!state.can_fetch());

		// On completion, the pending channel is now deployed and be fetched from again.
		assert!(state.on_fetch_completed());
		assert_eq!(state, DeploymentStatus::Deployed);
		assert!(state.can_fetch());
		expect_deposit_state!(state, ETH, EvmFetchId::NotRequired);
		expect_deposit_state!(state, USDC, EvmFetchId::Fetch(..));

		// Channel is now in its final deployed state and be fetched from at any time.
		assert!(!state.on_fetch_scheduled());
		assert!(state.can_fetch());
		assert!(!state.on_fetch_completed());
		assert!(state.can_fetch());
		expect_deposit_state!(state, ETH, EvmFetchId::NotRequired);
		expect_deposit_state!(state, USDC, EvmFetchId::Fetch(..));

		assert_eq!(state, DeploymentStatus::Deployed);
		assert!(!state.on_fetch_scheduled());
	}
}
