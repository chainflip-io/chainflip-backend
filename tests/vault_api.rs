use serde::Serialize;

use reqwest::StatusCode;

use blockswap::{
    common,
    side_chain::MemorySideChain,
    utils::test_utils::get_transactions_provider,
    vault::api::{v1::post_quote::QuoteResponse, APIServer},
};
use std::sync::{Arc, Mutex};

type QuoteResponseWrapped = common::api::Response<QuoteResponse>;

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
        .json::<common::api::Response<QuoteResponse>>()
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

#[tokio::test]
async fn vault_http_server_tests() {
    let side_chain = MemorySideChain::new();
    let side_chain = Arc::new(Mutex::new(side_chain));

    let provider = get_transactions_provider();
    let provider = Arc::new(Mutex::new(provider));

    let (tx, rx) = tokio::sync::oneshot::channel();

    let thread_handle = std::thread::spawn(move || {
        APIServer::serve(side_chain, provider, rx);
    });

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

    // TODO:  Add once completed

    // shutdown the server
    let _ = tx.send(());

    thread_handle.join().unwrap();
}
