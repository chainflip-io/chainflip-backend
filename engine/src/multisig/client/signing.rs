pub mod signing_data;
pub mod signing_detail;
pub mod signing_stages;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use crate::multisig::{crypto::ECPoint, MessageHash};

use super::common::KeygenResult;

#[cfg(test)]
pub use signing_data::{gen_signing_data_stage1, gen_signing_data_stage4};

pub use signing_data::{Comm1, LocalSig3, VerifyComm2, VerifyLocalSig4};

/// Data common for signing stages
#[derive(Clone)]
pub struct SigningStateCommonInfo<P: ECPoint> {
	pub data: MessageHash,
	pub key: Arc<KeygenResult<P>>,
}
