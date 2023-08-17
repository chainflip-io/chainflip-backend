use super::*;
use frame_support::traits::ConstBool;

/// A Chain that can't be constructed.
#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum NoneChain {}

impl IntoForeignChainAddress<NoneChain> for ForeignChainAddress {
	fn into_foreign_chain_address(address: ForeignChainAddress) -> ForeignChainAddress {
		address
	}
}

impl Chain for NoneChain {
	const NAME: &'static str = "NONE";
	type KeyHandoverIsRequired = ConstBool<false>;
	type OptimisticActivation = ConstBool<true>;
	type ChainBlockNumber = u64;
	type ChainAmount = u64;
	type TransactionFee = u64;
	type TrackedData = ();
	type ChainAsset = assets::any::Asset;
	type ChainAccount = ForeignChainAddress;
	type EpochStartData = ();
	type DepositFetchId = ChannelId;
	type DepositChannelState = ();
	type DepositDetails = ();
}

impl ChainCrypto for NoneChain {
	type AggKey = ();
	type Payload = ();
	type ThresholdSignature = ();
	type TransactionInId = ();
	type TransactionOutId = ();
	type GovKey = ();

	fn verify_threshold_signature(
		_agg_key: &Self::AggKey,
		_payload: &Self::Payload,
		_signature: &Self::ThresholdSignature,
	) -> bool {
		unimplemented!()
	}

	fn agg_key_to_payload(_agg_key: Self::AggKey) -> Self::Payload {
		unimplemented!()
	}
}
