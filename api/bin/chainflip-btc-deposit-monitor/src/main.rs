#![feature(async_fn_traits)]

// mod btc2;
mod btc3;
mod btc4;
mod elliptic;
mod monitor_provider;
mod targets;

use std::{env, str::FromStr, sync::{Arc, Mutex}};
// use btc2::monitor_mempool;
use btc3::start_monitor;
use btc4::call_monitor;
use cf_chains::{assets::btc, Bitcoin};
use chainflip_api::settings::HttpBasicAuthEndpoint;
use elliptic::EllipticClient;
use monitor_provider::monitor2;


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

	let addresses = match env::var("BTC_TARGET_ADDRESS") {
		Ok(a) => {
			let address = bitcoin::Address::from_str(&a).expect("could not parse btc address").require_network(bitcoin::Network::Bitcoin).expect("could not validate address");
			vec![address]
		},
		Err(_) => Vec::new(),
	};

	// start_monitor(endpoint).await;
    call_monitor(endpoint, addresses).await;
}
