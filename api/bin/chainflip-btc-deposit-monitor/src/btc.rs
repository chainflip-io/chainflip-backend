

// struct BtcMonitor {
//     btc_client
// }

use chainflip_engine::{btc::retry_rpc::BtcRetryRpcClient, witness::btc::source::BtcSource};


pub fn start_monitor() {

    let btc_client: BtcRetryRpcClient = todo!();

	let btc_source = BtcSource::new(btc_client.clone()).strictly_monotonic().shared(scope);
}
