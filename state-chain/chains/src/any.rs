use crate::{
	address::ForeignChainAddress, none::NoneChainCrypto, Chain, FeeRefundCalculator,
	TransactionMetaDataHandler,
};

use cf_primitives::{
	chains::{assets, AnyChain},
	AssetAmount, ChannelId,
};

impl Chain for AnyChain {
	const NAME: &'static str = "AnyChain";
	type ChainCrypto = NoneChainCrypto;
	type ChainBlockNumber = u64;
	type ChainAmount = AssetAmount;
	type TransactionFee = Self::ChainAmount;
	type TrackedData = ();
	type ChainAsset = assets::any::Asset;
	type ChainAccount = ForeignChainAddress;
	type EpochStartData = ();
	type DepositFetchId = ChannelId;
	type DepositChannelState = ();
	type DepositDetails = ();
	type Transaction = ();
	type TransactionMetaData = ();
	type ReplayProtectionParams = ();
	type ReplayProtection = ();
}

impl FeeRefundCalculator<AnyChain> for () {
	fn return_fee_refund(
		&self,
		_fee_paid: <AnyChain as Chain>::TransactionFee,
	) -> <AnyChain as Chain>::ChainAmount {
		unimplemented!()
	}
}

impl TransactionMetaDataHandler<AnyChain> for () {
	fn extract_metadata(
		_transaction: &<AnyChain as Chain>::Transaction,
	) -> <AnyChain as Chain>::TransactionMetaData {
		unimplemented!()
	}

	fn verify_metadata(
		_metadata: &<AnyChain as Chain>::TransactionMetaData,
		_expected_metadata: &<AnyChain as Chain>::TransactionMetaData,
	) -> bool {
		unimplemented!()
	}
}
