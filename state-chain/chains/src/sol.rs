pub use cf_primitives::chains::Solana;

use cf_primitives::ChannelId;
use sp_core::{ConstBool, RuntimeDebug};
use sp_std::vec;

use sol_prim::SlotNumber;

use crate::{address, assets, FeeRefundCalculator, TypeInfo};
use codec::{Decode, Encode};

use super::{Chain, ChainCrypto};

pub mod api;
pub mod benchmarking;
pub mod consts;
pub mod instruction_builder;
mod tracked_data;

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

pub use tracked_data::SolTrackedData;

impl Chain for Solana {
	const NAME: &'static str = "Solana";
	const GAS_ASSET: Self::ChainAsset = assets::sol::Asset::Sol;

	type ChainCrypto = SolanaCrypto;
	type ChainBlockNumber = SlotNumber;
	type ChainAmount = SolAmount;
	type TransactionFee = Self::ChainAmount;
	type TrackedData = tracked_data::SolTrackedData;
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
