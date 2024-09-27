
mod btc;
mod elliptic;

use std::env;
use elliptic::EllipticClient;


#[tokio::main]
async fn main() {
	let hash = env::var("BTC_HASH").expect("need btc hash");
	let address = env::var("BTC_ADDRESS").expect("need btc address");
	EllipticClient::new().single_analysis(hash.into(), address.into(), "test_customer_1".into()).await.unwrap();
}
