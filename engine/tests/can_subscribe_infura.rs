use chainflip_engine::{
    eth::{
        key_manager::KeyManager,
        rpc::{EthHttpRpcClient, EthWsRpcClient},
        EthObserver,
    },
    logging::utils,
    settings::Settings,
};

use anyhow::Result;
use config::{Config, Environment, File};
use futures::stream::StreamExt;
use sp_core::H160;

/// Simply runs a test against infura to ensure we can subscribe to infura
#[tokio::test]
pub async fn test_all_key_manager_events() {
    let root_logger = utils::new_cli_logger();

    let settings = test_settings_from_file_and_env().unwrap();

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

fn test_settings_from_file_and_env() -> Result<Settings> {
    // Merge the configuration file and then the environment, overwrite anything that matches
    let s: Settings = Config::builder()
        .add_source(File::with_name("config/Testing.toml"))
        .add_source(Environment::default().separator("__"))
        .build()?
        .try_deserialize()?;

    // make sure the settings are clean
    s.validate_settings()?;

    Ok(s)
}

mod test {
    use crate::test_settings_from_file_and_env;

    #[test]
    fn test_init_config_from_file_and_env() {
        let eth_node_key = "ETH__NODE_ENDPOINT";
        let fake_endpoint = "ws://fake.rinkeby.endpoint/flippy1234";
        std::env::set_var(eth_node_key, fake_endpoint);

        let settings_with_env = test_settings_from_file_and_env().unwrap();

        // ensure the file and env settings *does* read environment vars
        assert_eq!(settings_with_env.eth.ws_node_endpoint, fake_endpoint);

        // clean up
        std::env::remove_var(eth_node_key);
    }
}
