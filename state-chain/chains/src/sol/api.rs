use core::marker::PhantomData;

use codec::{Decode, Encode};
use frame_support::{CloneNoBound, DebugNoBound, EqNoBound, PartialEqNoBound};
use scale_info::TypeInfo;
use sp_std::vec;

use crate::{ApiCall, ConsolidateCall, SetAggKeyWithAggKey};

use super::{Solana, SolanaCrypto};

#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(Env))]
pub struct SolanaApi<Env>(PhantomData<Env>);

impl<Env: 'static> ApiCall<SolanaCrypto> for SolanaApi<Env> {
	fn threshold_signature_payload(&self) -> <SolanaCrypto as crate::ChainCrypto>::Payload {
		unimplemented!()
	}

	fn signed(
		self,
		_threshold_signature: &<SolanaCrypto as crate::ChainCrypto>::ThresholdSignature,
	) -> Self {
		unimplemented!()
	}

	fn chain_encoded(&self) -> vec::Vec<u8> {
		unimplemented!()
	}

	fn is_signed(&self) -> bool {
		unimplemented!()
	}

	fn transaction_out_id(&self) -> <SolanaCrypto as crate::ChainCrypto>::TransactionOutId {
		unimplemented!()
	}
}

impl<Env: 'static> ConsolidateCall<Solana> for SolanaApi<Env> {
	fn consolidate_utxos() -> Result<Self, crate::ConsolidationError> {
		unimplemented!()
	}
}

impl<Env: 'static> SetAggKeyWithAggKey<SolanaCrypto> for SolanaApi<Env> {
	fn new_unsigned(
		_maybe_old_key: Option<<SolanaCrypto as crate::ChainCrypto>::AggKey>,
		_new_key: <SolanaCrypto as crate::ChainCrypto>::AggKey,
	) -> Result<Self, crate::SetAggKeyWithAggKeyError> {
		unimplemented!()
	}
}
