use serde::Serialize;

use reqwest::StatusCode;

use blockswap::{
    common::{self, *},
    utils::test_utils::staking::get_fake_staker,
    utils::test_utils::{self, *},
    vault::api::v1::post_swap::SwapQuoteResponse,
    vault::api::APIServer,
};
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

async fn post_unstake_req<T>(req: &T) -> StatusCode
where
    T: Serialize + ?Sized,
{
    let res = CLIENT
        .post("http://localhost:3030/v1/unstake")
        .json(req)
        .send()
        .await
        .unwrap();

    dbg!(&res);

    let status = res.status();
    let text = res.text().await;
    dbg!(&text);

    status
}

async fn check_unstake_endponit(config: &TestConfig) -> StatusCode {
    let timestamp = Timestamp::now().0.to_string();

    let req = serde_json::json!({
        "staker_id": config.staker.public_key(),
        "pool": "ETH",
        "loki_address": "T6UBx3DnXsocMxGDgLR9ejGmbY5iphPiG9YwDZyNiCM81dgM776a1h7FwFCZZxm7yPabRxQeyfLesBynTWP6DfJq1DAtb6QYn",
        "other_address": "<PLACEHOLDER>",
        "timestamp": timestamp,
        "signature": "<PLACEHOLDER>",
    });
    post_unstake_req(&req).await
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
    // Add a valid stake

    let loki_amount = LokiAmount::from_decimal_string("1.0");
    let eth_amount = GenericCoinAmount::from_decimal_string(Coin::ETH, "2.0");

    let keypair = &config.staker;

    runner.add_witnessed_stake_tx(&keypair.id(), loki_amount, eth_amount);
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

    let chain = Arc::clone(&runner.chain);
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
        let status = send_blocks_req(&[("number", 0), ("limit", 1)]).await;
        assert_eq!(status, StatusCode::OK);
    }

    {
        // (no params)
        let status = send_blocks_req(&[("", "")]).await;
        assert_eq!(status, StatusCode::OK);
    }

    // POST requests

    {
        // v1/unstake
        let status = check_unstake_endponit(&config).await;
        assert_eq!(status, StatusCode::OK);
    }

    // ***********************
    // ******* CLEANUP *******
    // ***********************

    // shutdown the server
    let _ = tx.send(());

    thread_handle.join().unwrap();
}
