use chainflip::{
    common::{self, *},
    utils::test_utils::staking::get_fake_staker,
    utils::test_utils::{self, *},
    vault::api::v1::post_swap::SwapQuoteResponse,
    vault::api::APIServer,
};
use chainflip_common::types::coin::Coin;
use data::TestData;
use reqwest::StatusCode;
use serde::Serialize;
use std::sync::Arc;

type QuoteResponseWrapped = common::api::Response<SwapQuoteResponse>;

lazy_static::lazy_static! {
    static ref CLIENT: reqwest::Client = reqwest::Client::new();
}

async fn send_quote_req<T>(req: &T) -> (StatusCode, QuoteResponseWrapped)
where
    T: Serialize,
{
    let res = CLIENT
        .post("http://localhost:3030/v1/quote")
        .json(&req)
        .send()
        .await
        .unwrap();

    let status = res.status();

    let res = res
        .json::<common::api::Response<SwapQuoteResponse>>()
        .await
        .unwrap();

    (status, res)
}

async fn send_blocks_req<T>(query: &T) -> StatusCode
where
    T: Serialize + ?Sized,
{
    let res = CLIENT
        .get("http://localhost:3030/v1/blocks")
        .query(query)
        .send()
        .await
        .unwrap();

    res.status()
}

async fn post_withdraw_req<T>(req: &T) -> StatusCode
where
    T: Serialize + ?Sized,
{
    let res = CLIENT
        .post("http://localhost:3030/v1/withdraw")
        .json(req)
        .send()
        .await
        .unwrap();

    let status = res.status();
    let _text = res.text().await;

    status
}

async fn send_portions_req<T>(query: &T) -> StatusCode
where
    T: Serialize + ?Sized,
{
    let res = CLIENT
        .get("http://localhost:3030/v1/portions")
        .query(query)
        .send()
        .await
        .unwrap();

    let status = res.status();

    let _text = res.text().await;

    status
}

async fn check_withdraw_endpoint(config: &TestConfig) -> StatusCode {
    let mut tx = TestData::withdraw_request_for_staker(&config.staker, Coin::ETH);

    tx.sign(&config.staker.keys)
        .expect("failed to sign withdraw request");

    let req = serde_json::json!({
        "stakerId": config.staker.public_key(),
        "pool": "ETH",
        "baseAddress": tx.base_address.to_string(),
        "otherAddress": tx.other_address.to_string(),
        "timestamp": tx.timestamp.to_string(),
        "fraction": tx.fraction,
        "signature": base64::encode(tx.signature),
    });

    post_withdraw_req(&req).await
}

struct TestConfig {
    /// A valid (known) staker
    pub staker: Staker,
}

impl TestConfig {
    /// Create an arbitrary instance
    fn default() -> Self {
        TestConfig {
            staker: get_fake_staker(),
        }
    }
}

/// Setup some state on the side chain for the tests to interact with
fn setup_state(config: &TestConfig, runner: &mut TestRunner) {
    // Add a valid deposit

    let loki_amount = LokiAmount::from_decimal_string("1.0");
    let eth_amount = GenericCoinAmount::from_decimal_string(Coin::ETH, "2.0");

    let keypair = &config.staker;

    runner.add_witnessed_deposit_quote(&keypair.id(), loki_amount, eth_amount);
    runner.sync();
}

/// Note that we reuse the same server instance in all of these tests
/// to reduce the overhead and preventing potential "Address already in use"
/// errors when tests are running in parallel
#[tokio::test]
async fn vault_http_server_tests() {
    // ***********************
    // ******** SETUP ********
    // ***********************

    test_utils::logging::init();

    let mut runner = TestRunner::new();

    let chain = Arc::clone(&runner.store);
    let provider = Arc::clone(&runner.provider);

    let (tx, rx) = tokio::sync::oneshot::channel();

    let thread_handle = std::thread::spawn(move || {
        APIServer::serve(&get_fake_config(), chain, provider, rx);
    });

    let config = TestConfig::default();

    setup_state(&config, &mut runner);

    // ***********************
    // ******** TESTS ********
    // ***********************

    {
        // number=0&limit=1
        let status = send_blocks_req(&[("number", 0u32), ("limit", 1u32)]).await;
        assert_eq!(status, StatusCode::OK);
    }

    {
        // (no params)
        let status = send_blocks_req(&[("", "")]).await;
        assert_eq!(status, StatusCode::OK);
    }

    {
        let staker_id = config.staker.public_key();
        let pool = "ETH".to_owned();
        let status = send_portions_req(&[("stakerId", staker_id), ("pool", pool)]).await;
        assert_eq!(status, StatusCode::OK);
    }

    // POST requests

    {
        // v1/withdraw
        let status = check_withdraw_endpoint(&config).await;
        assert_eq!(status, StatusCode::OK);
    }

    // ***********************
    // ******* CLEANUP *******
    // ***********************

    // shutdown the server
    let _ = tx.send(());

    thread_handle.join().unwrap();
}
