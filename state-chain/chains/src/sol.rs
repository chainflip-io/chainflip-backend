pub use cf_primitives::chains::Solana;
use cf_primitives::{AssetAmount, ChannelId};

use sp_core::ConstBool;
use sp_std::vec;

use sol_prim::SlotNumber;

use crate::{address, assets, FeeRefundCalculator, TypeInfo,FeeEstimationApi};
use codec::{Decode, Encode, MaxEncodedLen};
use serde::{Deserialize, Serialize};

use super::{Chain, ChainCrypto};

pub mod api;
pub mod benchmarking;
pub mod consts;

pub use sol_prim::{
	pda::{Pda as DerivedAddressBuilder, PdaError as AddressDerivationError},
	Address as SolAddress, Digest as SolHash, Signature as SolSignature,
};

impl Chain for Solana {
	const NAME: &'static str = "Solana";
	const GAS_ASSET: Self::ChainAsset = assets::sol::Asset::Sol;

	type ChainCrypto = SolanaCrypto;
	type ChainBlockNumber = SlotNumber;
	type ChainAmount = AssetAmount;
	type TransactionFee = Self::ChainAmount;
	type TrackedData = SolTrackedData;
	type ChainAsset = assets::sol::Asset;
	type ChainAccount = SolAddress;
	type EpochStartData = (); //todo
	type DepositFetchId = ChannelId;
	type DepositChannelState = (); //todo
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
	type Payload = (); //todo
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

pub const LAMPORTS_PER_SIGNATURE: <Solana as Chain>::ChainAmount = 5000;

// This is to be used both for ingress/egress estimation and for setting the compute units
// limit when crafting transactions
mod compute_units_costs {
	pub const BASE_COMPUTE_UNITS_PER_TX: u128 = 450;
	pub const COMPUTE_UNITS_PER_FETCH_NATIVE: u128 = 7_500;
	pub const COMPUTE_UNITS_PER_FETCH_TOKEN: u128 = 31_000;
	pub const COMPUTE_UNITS_PER_TRANSFER_NATIVE: u128 = 300;
	pub const COMPUTE_UNITS_PER_TRANSFER_TOKEN: u128 = 41_200;
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

		let compute_units_per_transfer = BASE_COMPUTE_UNITS_PER_TX +
			match asset {
				assets::sol::Asset::Sol => COMPUTE_UNITS_PER_FETCH_NATIVE,
				// TODO: To add when USDC is supported
				// assets::sol::Asset::SolUsdc => COMPUTE_UNITS_PER_FETCH_TOKEN,
			};

		LAMPORTS_PER_SIGNATURE + (self.priority_fee).saturating_mul(compute_units_per_transfer)
	}
}

#[derive(
	Debug,
	Clone,
	Copy,
	PartialEq,
	Eq,
	TypeInfo,
	Encode,
	Decode,
	MaxEncodedLen,
	Serialize,
	Deserialize,
)]
pub struct SolTransaction {}

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
