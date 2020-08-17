use blockswap::{
    common,
    side_chain::FakeSideChain,
    vault::api::{APIServer, QuoteQueryResponse},
};
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn vault_http_server_tests() {
    let side_chain = FakeSideChain::new();
    let side_chain = Arc::new(Mutex::new(side_chain));

    let (tx, rx) = tokio::sync::oneshot::channel();

    let thread_handle = std::thread::spawn(move || {
        APIServer::serve(side_chain, rx);
    });

    let client = reqwest::Client::new();

    let res = client
        .get("http://localhost:3030/v1/blocks?number=0&limit=1")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), reqwest::StatusCode::OK);

    let res = client
        .get("http://localhost:3030/v1/blocks")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), reqwest::StatusCode::OK);

    // POST requests

    // Missing fields
    let req_body = serde_json::json!({
        "inputCoin": "Loki",
        "inputReturnAddress": "TODO"
    });

    let res = client
        .post("http://localhost:3030/v1/quote")
        .body(req_body.to_string())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), reqwest::StatusCode::BAD_REQUEST);

    let res = res
        .json::<common::api::Response<QuoteQueryResponse>>()
        .await
        .unwrap();

    assert_eq!(res.success, false);
    assert_eq!(&res.error.unwrap().message, "field missing: inputAddressID");

    // shutdown the server
    let _ = tx.send(());

    thread_handle.join().unwrap();
}
