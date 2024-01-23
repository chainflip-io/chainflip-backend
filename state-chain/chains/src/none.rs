use super::*;
use frame_support::traits::ConstBool;

/// A Chain that can't be constructed.
#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum NoneChain {}

impl Chain for NoneChain {
	const NAME: &'static str = "NONE";
	const GAS_ASSET: Self::ChainAsset = assets::any::Asset::Usdc;
	type ChainCrypto = NoneChainCrypto;
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
	type Transaction = ();
	type TransactionMetadata = ();
	type ReplayProtectionParams = ();
	type ReplayProtection = ();
}

impl FeeRefundCalculator<NoneChain> for () {
	fn return_fee_refund(
		&self,
		_fee_paid: <NoneChain as Chain>::TransactionFee,
	) -> <NoneChain as Chain>::ChainAmount {
		unimplemented!()
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoneChainCrypto;
impl ChainCrypto for NoneChainCrypto {
	type UtxoChain = ConstBool<false>;
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

	fn agg_key_to_payload(_agg_key: Self::AggKey, _for_handover: bool) -> Self::Payload {
		unimplemented!()
	}

	fn maybe_broadcast_barriers_on_rotation(
		_rotation_broadcast_id: BroadcastId,
	) -> Vec<BroadcastId> {
		unimplemented!()
	}
}
