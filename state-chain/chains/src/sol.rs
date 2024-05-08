pub use cf_primitives::chains::Solana;

use cf_primitives::ChannelId;
use sp_core::{ConstBool, RuntimeDebug};
use sp_std::{vec, vec::Vec};

use sol_prim::SlotNumber;

use crate::{address, assets, FeeEstimationApi, FeeRefundCalculator, TypeInfo};
use codec::{Decode, Encode, MaxEncodedLen};
use serde::{Deserialize, Serialize};

use super::{Chain, ChainCrypto};

pub mod api;
pub mod benchmarking;
pub mod consts;
pub mod instruction_builder;
pub mod sol_tx_core;

pub use crate::assets::sol::Asset as SolAsset;
pub use sol_prim::{
	pda::{Pda as DerivedAddressBuilder, PdaError as AddressDerivationError},
	Address as SolAddress, Amount as SolAmount, ComputeLimit as SolComputeLimit, Digest as SolHash,
	Signature as SolSignature,
};
pub use sol_tx_core::{
	AccountMeta as SolAccountMeta, Hash as RawSolHash, Instruction as SolInstruction,
	Message as SolMessage, Pubkey as SolPubkey, Transaction as SolTransaction,
};

impl Chain for Solana {
	const NAME: &'static str = "Solana";
	const GAS_ASSET: Self::ChainAsset = assets::sol::Asset::Sol;
	const WITNESS_PERIOD: Self::ChainBlockNumber = 15;

	type ChainCrypto = SolanaCrypto;
	type ChainBlockNumber = SlotNumber;
	type ChainAmount = SolAmount;
	type TransactionFee = Self::ChainAmount;
	type TrackedData = SolTrackedData;
	type ChainAsset = assets::sol::Asset;
	type ChainAccount = SolAddress;
	type EpochStartData = (); //todo
	type DepositFetchId = ChannelId;
	type DepositChannelState = SolanaDepositChannelState; //todo
	type DepositDetails = (); //todo
	type Transaction = SolTransaction;
	type TransactionMetadata = (); //todo
	type ReplayProtectionParams = (); //todo
	type ReplayProtection = (); //todo
	type TransactionRef = SolHash;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SolanaCrypto;

impl ChainCrypto for SolanaCrypto {
	type UtxoChain = ConstBool<false>;
	type KeyHandoverIsRequired = ConstBool<false>;

	type AggKey = SolAddress;
	type Payload = SolMessage; //todo
	type ThresholdSignature = SolSignature;
	type TransactionInId = SolHash;
	type TransactionOutId = Self::ThresholdSignature;

	type GovKey = SolAddress;

	fn verify_threshold_signature(
		_agg_key: &Self::AggKey,
		_payload: &Self::Payload,
		_signature: &Self::ThresholdSignature,
	) -> bool {
		todo!()
	}

	fn agg_key_to_payload(_agg_key: Self::AggKey, _for_handover: bool) -> Self::Payload {
		todo!()
	}

	fn handover_key_matches(_current_key: &Self::AggKey, _new_key: &Self::AggKey) -> bool {
		todo!()
	}

	fn key_handover_is_required() -> bool {
		todo!()
	}

	fn maybe_broadcast_barriers_on_rotation(
		_rotation_broadcast_id: cf_primitives::BroadcastId,
	) -> vec::Vec<cf_primitives::BroadcastId> {
		todo!()
	}
}

pub const LAMPORTS_PER_SIGNATURE: SolAmount = 5000u64;

// This is to be used both for ingress/egress estimation and for setting the compute units
// limit when crafting transactions by the State Chain.
mod compute_units_costs {
	use super::SolAmount;
	pub const BASE_COMPUTE_UNITS_PER_TX: SolAmount = 450u64;
	pub const COMPUTE_UNITS_PER_FETCH_NATIVE: SolAmount = 7_500u64;
	pub const COMPUTE_UNITS_PER_TRANSFER_NATIVE: SolAmount = 300u64;
	#[allow(dead_code)]
	pub const COMPUTE_UNITS_PER_FETCH_TOKEN: SolAmount = 31_000u64;
	#[allow(dead_code)]
	pub const COMPUTE_UNITS_PER_TRANSFER_TOKEN: SolAmount = 41_200u64;
}

#[derive(
	Default,
	Clone,
	Encode,
	Decode,
	MaxEncodedLen,
	TypeInfo,
	Debug,
	PartialEq,
	Eq,
	Serialize,
	Deserialize,
)]
pub struct SolTrackedData {
	pub priority_fee: <Solana as Chain>::ChainAmount,
}

impl FeeEstimationApi<Solana> for SolTrackedData {
	fn estimate_egress_fee(
		&self,
		asset: <Solana as crate::Chain>::ChainAsset,
	) -> <Solana as crate::Chain>::ChainAmount {
		use compute_units_costs::*;

		let compute_units_per_transfer = BASE_COMPUTE_UNITS_PER_TX +
			match asset {
				assets::sol::Asset::Sol => COMPUTE_UNITS_PER_TRANSFER_NATIVE,
				// TODO: To add when USDC is supported
				// assets::sol::Asset::SolUsdc => COMPUTE_UNITS_PER_TRANSFER_TOKEN,
			};

		LAMPORTS_PER_SIGNATURE + (self.priority_fee).saturating_mul(compute_units_per_transfer)
	}
	fn estimate_ingress_fee(
		&self,
		asset: <Solana as crate::Chain>::ChainAsset,
	) -> <Solana as crate::Chain>::ChainAmount {
		use compute_units_costs::*;

		let compute_units_per_fetch = BASE_COMPUTE_UNITS_PER_TX +
			match asset {
				assets::sol::Asset::Sol => COMPUTE_UNITS_PER_FETCH_NATIVE,
				// TODO: To add when USDC is supported
				// assets::sol::Asset::SolUsdc => COMPUTE_UNITS_PER_FETCH_TOKEN,
			};

		LAMPORTS_PER_SIGNATURE + (self.priority_fee).saturating_mul(compute_units_per_fetch)
	}
}

impl FeeRefundCalculator<Solana> for SolTransaction {
	fn return_fee_refund(
		&self,
		fee_paid: <Solana as Chain>::TransactionFee,
	) -> <Solana as Chain>::ChainAmount {
		fee_paid
	}
}

impl TryFrom<address::ForeignChainAddress> for SolAddress {
	type Error = address::AddressError;
	fn try_from(value: address::ForeignChainAddress) -> Result<Self, Self::Error> {
		if let address::ForeignChainAddress::Sol(value) = value {
			Ok(value)
		} else {
			Err(address::AddressError::InvalidAddress)
		}
	}
}
impl From<SolAddress> for address::ForeignChainAddress {
	fn from(value: SolAddress) -> Self {
		address::ForeignChainAddress::Sol(value)
	}
}

impl address::ToHumanreadableAddress for SolAddress {
	#[cfg(feature = "std")]
	type Humanreadable = String;

	#[cfg(feature = "std")]
	fn to_humanreadable(
		&self,
		_network_environment: cf_primitives::NetworkEnvironment,
	) -> Self::Humanreadable {
		self.to_string()
	}
}

#[derive(Clone, Encode, Decode, TypeInfo, Eq, PartialEq, RuntimeDebug)]
pub struct SolanaDepositChannelState {
	pub seed: Vec<u8>,
	pub bump: u8,
}

impl crate::ChannelLifecycleHooks for SolanaDepositChannelState {}
