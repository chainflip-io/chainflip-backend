use chainflip_engine::{
    eth::{
        key_manager::{ChainflipKey, KeyManager, KeyManagerEvent},
        rpc::{EthHttpRpcClient, EthWsRpcClient},
        EthObserver,
    },
    logging::utils,
    settings::{CommandLineOptions, Settings},
};

use futures::stream::StreamExt;
use sp_core::H160;
use std::str::FromStr;
use web3::types::U256;

mod common;
use crate::common::IntegrationTestSettings;

#[tokio::test]
pub async fn test_all_key_manager_events() {
    let root_logger = utils::new_cli_logger();

    let integration_test_settings =
        IntegrationTestSettings::from_file("tests/config.toml").unwrap();
    let settings =
        Settings::from_file_and_env("config/Testing.toml", CommandLineOptions::default()).unwrap();

    let eth_ws_rpc_client = EthWsRpcClient::new(&settings.eth, &root_logger)
        .await
        .expect("Couldn't create EthWsRpcClient");

    let eth_http_rpc_client = EthHttpRpcClient::new(&settings.eth, &root_logger)
        .expect("Couldn't create EthHttpRpcClient");

    let key_manager = KeyManager::new(integration_test_settings.eth.key_manager_address);

    // The stream is infinite unless we stop it after a short time
    // in which it should have already done it's job.
    let km_events = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        key_manager.block_stream(eth_ws_rpc_client, eth_http_rpc_client, 0, &root_logger),
    )
    .await
    .expect(common::EVENT_STREAM_TIMEOUT_MESSAGE)
    .unwrap()
    .map(|block| futures::stream::iter(block.events))
    .flatten()
    .take_until(tokio::time::sleep(std::time::Duration::from_millis(1000)))
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .collect::<Vec<_>>();

    assert!(
        !km_events.is_empty(),
        "{}",
        common::EVENT_STREAM_EMPTY_MESSAGE
    );

    // The following event details correspond to the events in chainflip-eth-contracts/scripts/deploy_and.py
    // All the key strings in this test are decimal pub keys derived from the priv keys in the consts.py script
    // https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/tests/consts.py

    km_events
            .iter()
            .find(|event| match &event.event_parameters {
            KeyManagerEvent::AggKeySetByAggKey {
                old_agg_key, new_agg_key
            } => {
                assert_eq!(old_agg_key,&ChainflipKey::from_dec_str("22479114112312168431982914496826057754130808976066989807481484372215659188398",true).unwrap());
                assert_eq!(new_agg_key,&ChainflipKey::from_dec_str("10521316663921629387264629518161886172223783929820773409615991397525613232925",true).unwrap());
                true
            },
            _ => false,
        }).expect("Didn't find AggKeySetByAggKey event");

    km_events
        .iter()
        .find(|event| match &event.event_parameters {
            KeyManagerEvent::AggKeySetByGovKey {
                old_agg_key, new_agg_key
            } => {
                if old_agg_key == &ChainflipKey::from_dec_str("10521316663921629387264629518161886172223783929820773409615991397525613232925",true).unwrap()
                || old_agg_key == &ChainflipKey::from_dec_str("22479114112312168431982914496826057754130808976066989807481484372215659188398",true).unwrap(){
                    assert_eq!(new_agg_key,&ChainflipKey::from_dec_str("22479114112312168431982914496826057754130808976066989807481484372215659188398",true).unwrap());
                }else{
                    panic!("Unexpected AggKeySetByGovKey event. The details did not match the 2 expected AggKeySetByGovKey events");
                }
                true
            },
            _ => false,
        }).expect("Didn't find AggKeySetByGovKey event");

    km_events
        .iter()
        .find(|event| match &event.event_parameters {
            KeyManagerEvent::GovKeySetByGovKey {
                old_gov_key,
                new_gov_key,
            } => {
                assert_eq!(
                    old_gov_key,
                    &H160::from_str("0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266").unwrap()
                );
                assert_eq!(
                    new_gov_key,
                    &H160::from_str("0x9965507d1a55bcc2695c58ba16fb37d819b0a4dc").unwrap()
                );
                true
            }
            _ => false,
        })
        .expect("Didn't find GovKeySetByGovKey event");

    km_events
        .iter()
        .find(|event| match &event.event_parameters {
            KeyManagerEvent::SignatureAccepted { sig_data, signer } => {
                assert_eq!(
                    sig_data.key_man_addr,
                    integration_test_settings.eth.key_manager_address
                );
                assert_eq!(sig_data.chain_id, U256::from_dec_str("31337").unwrap());
                assert_eq!(sig_data.nonce, U256::from_dec_str("0").unwrap());
                assert_eq!(
                    signer,
                    &H160::from_str("0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266").unwrap()
                );
                true
            }
            _ => false,
        })
        .expect("Didn't find SignatureAccepted event");
}
