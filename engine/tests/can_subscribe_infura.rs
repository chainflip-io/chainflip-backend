use chainflip_engine::{
    eth::{key_manager::KeyManager, new_synced_web3_client},
    logging::utils,
    settings::Settings,
};

use futures::stream::StreamExt;

mod common;

/// Simply runs a test against infura to ensure we can subscribe to infura
#[tokio::test]
pub async fn test_all_key_manager_events() {
    let root_logger = utils::create_cli_logger();

    let settings = Settings::from_file_and_env("config/Testing.toml").unwrap();

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
        .take_until(tokio::time::sleep(std::time::Duration::from_millis(5)))
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .expect("Error in event stream");
}
