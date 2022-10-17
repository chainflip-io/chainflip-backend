pub mod signing_detail;
pub mod signing_stages;

use std::sync::Arc;

use crate::multisig::{crypto::ECPoint, MessageHash};

use super::common::KeygenResult;

/// Data common for signing stages
#[derive(Clone)]
pub struct SigningStateCommonInfo<P: ECPoint> {
	pub data: MessageHash,
	pub key: Arc<KeygenResult<P>>,
}
