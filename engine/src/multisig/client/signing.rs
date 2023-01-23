mod signing_data;
mod signing_detail;
mod signing_stages;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use crate::multisig::CryptoScheme;

use super::common::KeygenResult;

pub use signing_data::{
	Comm1, LocalSig3, SigningCommitment, SigningData, VerifyComm2, VerifyLocalSig4,
};

pub use signing_detail::generate_schnorr_response;

pub use signing_stages::AwaitCommitments1;

#[cfg(test)]
pub use signing_data::{gen_signing_data_stage1, gen_signing_data_stage2, gen_signing_data_stage4};

#[cfg(test)]
pub use signing_detail::get_lagrange_coeff;

/// Data common for signing stages
#[derive(Clone)]
pub struct SigningStateCommonInfo<C: CryptoScheme> {
	pub payload: C::SigningPayload,
	pub key: Arc<KeygenResult<C>>,
}
