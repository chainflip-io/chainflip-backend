use sp_core::ConstBool;
use sp_std::vec;

use crate::ChainCrypto;

use super::{SolPublicKey, SolSignature};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SolanaCrypto;

impl ChainCrypto for SolanaCrypto {
	type UtxoChain = ConstBool<false>;
	type KeyHandoverIsRequired = ConstBool<false>;

	type AggKey = SolPublicKey;
	type Payload = [u8; 0];
	type ThresholdSignature = SolSignature;
	type TransactionInId = [u8; 0];
	type TransactionOutId = Self::ThresholdSignature;

	type GovKey = SolPublicKey;

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

	fn handover_key_matches(_current_key: &Self::AggKey, _new_key: &Self::AggKey) -> bool {
		unimplemented!()
	}

	fn key_handover_is_required() -> bool {
		unimplemented!()
	}

	fn maybe_broadcast_barriers_on_rotation(
		_rotation_broadcast_id: cf_primitives::BroadcastId,
	) -> vec::Vec<cf_primitives::BroadcastId> {
		unimplemented!()
	}
}
