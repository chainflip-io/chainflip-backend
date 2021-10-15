use chainflip_engine::{
    eth::{key_manager::KeyManager, new_synced_web3_client},
    logging::utils,
    settings::Settings,
};

use anyhow::Result;
use config::{Config, Environment, File};
use futures::stream::StreamExt;

mod common;

/// Simply runs a test against infura to ensure we can subscribe to infura
#[tokio::test]
pub async fn test_all_key_manager_events() {
    let root_logger = utils::create_cli_logger();

    let settings = test_settings_from_file_and_env().unwrap();

    let web3 = new_synced_web3_client(&settings, &root_logger)
        .await
        .unwrap();

    let key_manager = KeyManager::new(&settings).unwrap();

    // The stream is infinite unless we stop it after a short time
    // in which it should have already done it's job.
    key_manager
        .event_stream(&web3, settings.eth.from_block, &root_logger)
        .await
        .unwrap()
        .take_until(tokio::time::sleep(std::time::Duration::from_millis(10)))
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .expect("Error in event stream");
}

fn test_settings_from_file_and_env() -> Result<Settings> {
    let mut s = Config::new();

    // merging in the configuration file
    s.merge(File::with_name("config/Testing.toml"))?;

    // merge in the environment, overwrite anything that matches
    s.merge(Environment::new().separator("__"))?;

    // You can deserialize (and thus freeze) the entire configuration as
    let s: Settings = s.try_into()?;

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
        assert_eq!(settings_with_env.eth.node_endpoint, fake_endpoint);

        // clean up
        std::env::remove_var(eth_node_key);
    }
}
