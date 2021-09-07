pub mod eth;

use super::ChainId;

pub struct Ethereum;
pub type EthereumInstance = frame_support::instances::Instance0;

impl From<Ethereum> for ChainId {
	fn from(_: Ethereum) -> Self {
		ChainId::Ethereum
	}
}