use chainflip_engine::{
    eth::{
        key_manager::KeyManager,
        rpc::{EthHttpRpcClient, EthWsRpcClient},
        EthObserver,
    },
    logging::utils,
    settings::{CommandLineOptions, Settings},
};

use futures::stream::StreamExt;
use sp_core::H160;

/// Simply runs a test against infura to ensure we can subscribe to infura
#[tokio::test]
pub async fn test_all_key_manager_events() {
    let root_logger = utils::new_cli_logger();

    let settings =
        Settings::from_file_and_env("config/Testing.toml", CommandLineOptions::default()).unwrap();

    let eth_ws_rpc_client = EthWsRpcClient::new(&settings.eth, &root_logger)
        .await
        .expect("Couldn't create EthWsRpcClient");

    let eth_http_rpc_client = EthHttpRpcClient::new(&settings.eth, &root_logger)
        .expect("Couldn't create EthHttpRpcClient");

    let key_manager = KeyManager::new(H160::default(), eth_ws_rpc_client.clone()).unwrap();

    // The stream is infinite unless we stop it after a short time
    // in which it should have already done it's job.
    let _key_manager_events = key_manager
        .event_stream(eth_ws_rpc_client, eth_http_rpc_client, 0, &root_logger)
        .await
        .unwrap()
        .take_until(tokio::time::sleep(std::time::Duration::from_millis(10)))
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Vec<_>>();
}
