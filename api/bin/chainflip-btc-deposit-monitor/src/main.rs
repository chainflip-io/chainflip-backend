
mod btc2;
mod elliptic;

use std::{env, sync::{Arc, Mutex}};
use btc2::monitor_mempool;
use cf_chains::assets::btc;
use elliptic::EllipticClient;


#[tokio::main]
async fn main() {
	// let hash = env::var("BTC_HASH").expect("need btc hash");
	// let address = env::var("BTC_ADDRESS").expect("need btc address");
	// EllipticClient::new().single_analysis(hash.into(), address.into(), "test_customer_1".into()).await.unwrap();

	let btc_endpoint = env::var("BTC_ENDPOINT").expect("need btc endpoint");
	let vaults = Arc::new(Mutex::new(Vec::new()));
	monitor_mempool(btc_endpoint, vaults).await;

}
