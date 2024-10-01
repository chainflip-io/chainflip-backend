
// mod btc2;
mod btc3;
mod elliptic;

use std::{env, sync::{Arc, Mutex}};
// use btc2::monitor_mempool;
use btc3::start_monitor;
use cf_chains::assets::btc;
use chainflip_api::settings::HttpBasicAuthEndpoint;
use elliptic::EllipticClient;


#[tokio::main]
async fn main() {
	// let hash = env::var("BTC_HASH").expect("need btc hash");
	// let address = env::var("BTC_ADDRESS").expect("need btc address");
	// EllipticClient::new().single_analysis(hash.into(), address.into(), "test_customer_1".into()).await.unwrap();

	// let btc_endpoint = env::var("BTC_ENDPOINT").expect("need btc endpoint");
	// let vaults = Arc::new(Mutex::new(Vec::new()));
	// monitor_mempool(btc_endpoint).await;


	let http_endpoint = env::var("BTC_HTTP_ENDPOINT").expect("need btc http endpoint");
	let basic_auth_user = env::var("BTC_AUTH_USER").expect("need btc auth user");
    let basic_auth_password = env::var("BTC_AUTH_PASSWORD").expect("need btc auth password");


    let endpoint = HttpBasicAuthEndpoint {
        http_endpoint: http_endpoint.into(),
        basic_auth_user: basic_auth_user.into(),
        basic_auth_password: basic_auth_password.into(),
    };
	start_monitor(endpoint).await;
}
