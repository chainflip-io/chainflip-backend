use cf_primitives::chains::{assets, Solana};
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};

use crate::{Chain, FeeEstimationApi};

pub const BASE_FEE: <Solana as Chain>::ChainAmount = 5000 /* lamports */;

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
pub struct SolanaTrackedData {
	pub priority_fee: <Solana as Chain>::ChainAmount,
}

mod fees {
	// TODO: In Solana we could treat NATIVE & TOKEN the same
	pub const BASE_COMPUTE_UNITS_PER_BATCH: u128 = 1; // TODO: Update this
	pub const COMPUTE_UNITS_PER_FETCH_NATIVE: u128 = 1; // TODO: Update this
	pub const COMPUTE_UNITS_PER_FETCH_TOKEN: u128 = 1; // TODO: Update this
	pub const COMPUTE_UNITS_PER_TRANSFER_NATIVE: u128 = 1; // TODO: Update this
	pub const COMPUTE_UNITS_PER_TRANSFER_TOKEN: u128 = 1; // TODO: Update this
}

impl FeeEstimationApi<Solana> for SolanaTrackedData {
	fn estimate_egress_fee(
		&self,
		asset: <Solana as crate::Chain>::ChainAsset,
	) -> <Solana as crate::Chain>::ChainAmount {
		use fees::*;

		let compute_units_per_transfer = BASE_COMPUTE_UNITS_PER_BATCH +
			match asset {
				assets::sol::Asset::Sol => COMPUTE_UNITS_PER_TRANSFER_NATIVE,
				assets::sol::Asset::SolUsdc => COMPUTE_UNITS_PER_TRANSFER_TOKEN,
			};

		BASE_FEE + (self.priority_fee).saturating_mul(compute_units_per_transfer)
	}
	fn estimate_ingress_fee(
		&self,
		asset: <Solana as crate::Chain>::ChainAsset,
	) -> <Solana as crate::Chain>::ChainAmount {
		use fees::*;

		let compute_units_per_transfer = BASE_COMPUTE_UNITS_PER_BATCH +
			match asset {
				assets::sol::Asset::Sol => COMPUTE_UNITS_PER_FETCH_NATIVE,
				assets::sol::Asset::SolUsdc => COMPUTE_UNITS_PER_FETCH_TOKEN,
			};

		BASE_FEE + (self.priority_fee).saturating_mul(compute_units_per_transfer)
	}
}
