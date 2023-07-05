use super::*;

/// A Chain that can't be constructed.
#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum NoneChain {}

// #[derive(Encode, Decode, TypeInfo, Clone, PartialEq, Eq, Copy, Debug)]
// pub struct NoneChainDepositAddress;

// impl DepositChannel for NoneChainDepositAddress {
// 	type Address = ForeignChainAddress;
// 	type DepositFetchId = ();

// 	fn get_address(&self) -> Self::Address {
// 		todo!()
// 	}

// 	fn get_deposit_fetch_id(&self) -> Self::DepositFetchId {
// 		todo!()
// 	}

// 	fn new(_channel_id: u64, _address: Self::Address) -> Self {
// 		todo!()
// 	}
// }

impl Chain for NoneChain {
	const NAME: &'static str = "NONE";
	type ChainBlockNumber = u64;
	type ChainAmount = u64;
	type TransactionFee = u64;
	type TrackedData = ();
	type ChainAsset = assets::any::Asset;
	type ChainAccount = ForeignChainAddress;
	type EpochStartData = ();
	type DepositFetchId = ();
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
