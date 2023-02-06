pub mod rpc;
pub mod runtime_version_updater;
pub mod witnesser;

use rpc::DotRpcApi;
use subxt::{Config, PolkadotConfig};

use anyhow::Result;

pub struct DotBroadcaster<DotRpc>
where
	DotRpc: DotRpcApi,
{
	rpc: DotRpc,
}

impl<DotRpc> DotBroadcaster<DotRpc>
where
	DotRpc: DotRpcApi,
{
	pub fn new(rpc: DotRpc) -> Self {
		Self { rpc }
	}

	pub async fn send(&self, encoded_bytes: Vec<u8>) -> Result<<PolkadotConfig as Config>::Hash> {
		self.rpc.submit_raw_encoded_extrinsic(encoded_bytes).await
	}
}
