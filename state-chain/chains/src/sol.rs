pub use cf_primitives::chains::Solana;
use cf_primitives::{AssetAmount, ChannelId};

use crate::{assets, FeeRefundCalculator};

use super::Chain;

mod chain_crypto;
mod public_key;
mod signature;
mod tracked_data;
mod transaction;

pub mod api;
pub mod consts;

pub use chain_crypto::SolanaCrypto;
pub use public_key::SolAddress;
pub use signature::SolSignature;
pub use transaction::SolTransaction;

impl Chain for Solana {
	const NAME: &'static str = "Solana";
	const GAS_ASSET: Self::ChainAsset = assets::sol::Asset::Sol;

	type ChainCrypto = SolanaCrypto;
	type ChainBlockNumber = u64;
	type ChainAmount = AssetAmount;
	type TransactionFee = Self::ChainAmount;
	type TrackedData = tracked_data::SolTrackedData;
	type ChainAsset = assets::sol::Asset;
	type ChainAccount = SolAddress;
	type EpochStartData = ();
	type DepositFetchId = ChannelId;
	type DepositChannelState = ();
	type DepositDetails = ();
	type Transaction = SolTransaction;
	type TransactionMetadata = ();
	type ReplayProtectionParams = ();
	type ReplayProtection = ();
}

impl FeeRefundCalculator<Solana> for SolTransaction {
	fn return_fee_refund(
		&self,
		fee_paid: <Solana as Chain>::TransactionFee,
	) -> <Solana as Chain>::ChainAmount {
		fee_paid
	}
}
