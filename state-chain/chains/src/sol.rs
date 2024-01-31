use cf_primitives::{chains::Solana, AssetAmount, ChannelId};

use crate::{assets, none::NoneChainCrypto, FeeRefundCalculator, ForeignChainAddress};

use super::Chain;

mod sol_chain_crypto;
pub use sol_chain_crypto::SolTransaction;

impl Chain for Solana {
	const NAME: &'static str = "Solana";
	const GAS_ASSET: Self::ChainAsset = assets::sol::Asset::Sol;

	type ChainCrypto = NoneChainCrypto;
	type ChainBlockNumber = u64;
	type ChainAmount = AssetAmount;
	type TransactionFee = Self::ChainAmount;
	type TrackedData = ();
	type ChainAsset = assets::sol::Asset;
	type ChainAccount = ForeignChainAddress;
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
