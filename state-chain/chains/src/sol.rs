pub use cf_primitives::chains::Solana;
use cf_primitives::{AssetAmount, ChannelId};

use sp_core::ConstBool;
use sp_std::vec;

use sol_prim::SlotNumber;

use crate::{address, assets, FeeRefundCalculator, TypeInfo};
use codec::{Decode, Encode, MaxEncodedLen};
use serde::{Deserialize, Serialize};

use super::{Chain, ChainCrypto};

pub mod api;
pub mod benchmarking;
pub mod consts;
mod tracked_data;

pub mod bpf_loader_instructions;
pub mod compute_budget;
#[cfg(test)]
pub mod extra_types_for_testing;
pub mod program_instructions;
pub mod sol_tx_building_blocks;
pub mod token_instructions;

pub use sol_prim::{
	pda::{Pda as DerivedAddressBuilder, PdaError as AddressDerivationError},
	Address as SolAddress, Digest as SolHash, Signature as SolSignature,
};

pub use tracked_data::SolTrackedData;

impl Chain for Solana {
	const NAME: &'static str = "Solana";
	const GAS_ASSET: Self::ChainAsset = assets::sol::Asset::Sol;

	type ChainCrypto = SolanaCrypto;
	type ChainBlockNumber = SlotNumber;
	type ChainAmount = AssetAmount;
	type TransactionFee = Self::ChainAmount;
	type TrackedData = tracked_data::SolTrackedData;
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
