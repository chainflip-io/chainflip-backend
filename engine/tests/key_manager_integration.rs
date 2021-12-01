use chainflip_engine::{
    eth::{
        key_manager::{ChainflipKey, KeyManager, KeyManagerEvent},
        new_synced_web3_client, EthObserver,
    },
    logging::utils,
    settings::Settings,
};

use futures::stream::StreamExt;
use sp_core::H160;
use std::str::FromStr;
use web3::types::U256;

mod common;

#[tokio::test]
pub async fn test_all_key_manager_events() {
    let root_logger = utils::new_cli_logger();

    let settings = Settings::from_file("config/Testing.toml").unwrap();

    let web3 = new_synced_web3_client(&settings, &root_logger)
        .await
        .unwrap();

    // TODO: Get the address from environment variables, so we don't need to start the SC
    let key_manager =
        KeyManager::new(H160::from_str("0xe7f1725E7734CE288F8367e1Bb143E90bb3F0512").unwrap())
            .unwrap();

    // The stream is infinite unless we stop it after a short time
    // in which it should have already done it's job.
    let km_events = key_manager
        .event_stream(&web3, settings.eth.from_block, &root_logger)
        .await
        .unwrap()
        .take_until(tokio::time::sleep(std::time::Duration::from_millis(1)))
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .expect("Error in event stream");

    assert!(
        !km_events.is_empty(),
        "{}",
        common::EVENT_STREAM_EMPTY_MESSAGE
    );

    // The following event details correspond to the events in chainflip-eth-contracts/scripts/deploy_and.py
    // See if the key change event matches 1 of the 3 events in the 'deploy_and.py' script
    // All the key strings in this test are decimal pub keys derived from the priv keys in the consts.py script
    // https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/tests/consts.py

    km_events
            .iter()
            .find(|event| match &event.event_parameters {
            KeyManagerEvent::AggKeySetByAggKey {
                old_key, new_key
            } => {
                assert_eq!(old_key,&ChainflipKey::from_dec_str("22479114112312168431982914496826057754130808976066989807481484372215659188398",true).unwrap());
                assert_eq!(new_key,&ChainflipKey::from_dec_str("10521316663921629387264629518161886172223783929820773409615991397525613232925",true).unwrap());
                true
            },
            _ => false,
        }).expect("Didn't find AggKeySetByAggKey event");

    km_events
        .iter()
        .find(|event| match &event.event_parameters {
            KeyManagerEvent::AggKeySetByGovKey {
                old_key, new_key
            } => {
                if old_key == &ChainflipKey::from_dec_str("10521316663921629387264629518161886172223783929820773409615991397525613232925",true).unwrap() {
                    assert_eq!(new_key,&ChainflipKey::from_dec_str("22479114112312168431982914496826057754130808976066989807481484372215659188398",true).unwrap());
                }else if old_key == &ChainflipKey::from_dec_str("22479114112312168431982914496826057754130808976066989807481484372215659188398",true).unwrap() {
                    assert_eq!(new_key,&ChainflipKey::from_dec_str("22479114112312168431982914496826057754130808976066989807481484372215659188398",true).unwrap());
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
            KeyManagerEvent::GovKeySetByGovKey { old_key, new_key } => {
                assert_eq!(
                    old_key,
                    &H160::from_str("0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266").unwrap()
                );
                assert_eq!(
                    new_key,
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
            KeyManagerEvent::SignatureAccepted {
                sig_data,
                broadcaster,
            } => {
                assert_eq!(
                    sig_data.key_man_addr,
                    H160::from_str("0xe7f1725e7734ce288f8367e1bb143e90bb3f0512").unwrap()
                );
                assert_eq!(sig_data.chain_id, U256::from_dec_str("31337").unwrap());
                assert_eq!(sig_data.nonce, U256::from_dec_str("0").unwrap());
                assert_eq!(
                    broadcaster,
                    &H160::from_str("0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266").unwrap()
                );
                true
            }
            _ => false,
        })
        .expect("Didn't find SignatureAccepted event");
}
