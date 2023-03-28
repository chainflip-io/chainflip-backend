use self::rpc::BtcRpcApi;

pub mod rpc;
pub mod witnesser;
pub mod witnessing;

use anyhow::Result;
use bitcoincore_rpc::bitcoin::Txid;
pub struct BtcBroadcaster<BtcRpc>
where
	BtcRpc: BtcRpcApi,
{
	rpc: BtcRpc,
}

impl<BtcRpc> BtcBroadcaster<BtcRpc>
where
	BtcRpc: BtcRpcApi,
{
	pub fn new(rpc: BtcRpc) -> Self {
		Self { rpc }
	}

	pub async fn send(&self, transaction_bytes: Vec<u8>) -> Result<Txid> {
		self.rpc.send_raw_transaction(transaction_bytes)
	}
}
