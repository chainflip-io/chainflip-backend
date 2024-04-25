use cf_primitives::chains::Solana;
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};

use crate::{Chain, FeeEstimationApi};

#[allow(dead_code)]
pub const DEFAULT_TRANSACTION_FEE: <Solana as Chain>::ChainAmount = 5000 /* lamports */;

#[derive(
	Default,
	Clone,
	Encode,
	Decode,
	MaxEncodedLen,
	TypeInfo,
	Debug,
	PartialEq,
	Eq,
	Serialize,
	Deserialize,
)]
pub struct SolTrackedData {}

impl FeeEstimationApi<Solana> for SolTrackedData {
	fn estimate_egress_fee(
		&self,
		_asset: <Solana as crate::Chain>::ChainAsset,
	) -> <Solana as crate::Chain>::ChainAmount {
		todo!()
	}
	fn estimate_ingress_fee(
		&self,
		_asset: <Solana as crate::Chain>::ChainAsset,
	) -> <Solana as crate::Chain>::ChainAmount {
		todo!()
	}
}
