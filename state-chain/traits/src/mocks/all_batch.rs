use cf_chains::{
	AllBatch, ApiCall, ChainAbi, ChainCrypto, Ethereum, FetchAssetParams, TransferAssetParams,
};
use codec::{Decode, Encode};
use scale_info::TypeInfo;

#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct MockAllBatch {
	nonce: <Ethereum as ChainAbi>::ReplayProtection,
	fetch_params: Vec<FetchAssetParams<Ethereum>>,
	transfer_params: Vec<TransferAssetParams<Ethereum>>,
}

impl AllBatch<Ethereum> for MockAllBatch {
	fn new_unsigned(
		fetch_params: Vec<FetchAssetParams<Ethereum>>,
		transfer_params: Vec<TransferAssetParams<Ethereum>>,
	) -> Self {
		// TODO: Give it a nonce
		Self { nonce: Default::default(), fetch_params, transfer_params }
	}
}

impl ApiCall<Ethereum> for MockAllBatch {
	fn threshold_signature_payload(&self) -> <Ethereum as ChainCrypto>::Payload {
		unimplemented!()
	}

	fn signed(self, _threshold_signature: &<Ethereum as ChainCrypto>::ThresholdSignature) -> Self {
		unimplemented!()
	}

	fn chain_encoded(&self) -> Vec<u8> {
		unimplemented!()
	}

	fn is_signed(&self) -> bool {
		unimplemented!()
	}
}
